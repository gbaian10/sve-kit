#!/usr/bin/env bash
# Usage: export.sh <run-dir with recipe.json> [extra export-offline options].
# The container runs detached so it outlives this shell; sampling and the summary do not.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
compose=(docker compose -f "$here/compose.yaml")
run_dir=$(realpath "${1:?usage: export.sh <run-dir> [export-offline options]}")
shift
[[ -f $run_dir/recipe.json ]] || { echo "missing $run_dir/recipe.json" >&2; exit 2; }

# .env only fills what the caller left unset, so a shell override such as an isolated output root wins.
while IFS='=' read -r key value; do
  [[ $key =~ ^[A-Z_][A-Z0-9_]*$ ]] || continue
  [[ -v $key ]] || export "$key=$value"
done <"$here/.env"
HOST_UID=$(id -u)
HOST_GID=$(id -g)
export EXPORT_RUN_DIR=$run_dir HOST_UID HOST_GID
mkdir -p "$run_dir/tmp"

# Whole GiB only, so the host check below compares like with like before anything starts.
export EXPORT_MEMORY=${EXPORT_MEMORY:-16g}
if [[ ! $EXPORT_MEMORY =~ ^[1-9][0-9]*g$ ]]; then
  echo "EXPORT_MEMORY must be whole GiB such as 16g, got '$EXPORT_MEMORY'" >&2
  exit 2
fi
# Starting without room for the whole cap pushes the desktop into systemd-oomd's reach.
limit_gib=${EXPORT_MEMORY%g}
avail_gib=$(awk '/^MemAvailable:/ {print int($2 / 1048576)}' /proc/meminfo)
if ((avail_gib < limit_gib + 4)); then
  echo "only ${avail_gib} GiB available; need the ${limit_gib} GiB cap plus 4 GiB" >&2
  exit 3
fi

"${compose[@]}" build --quiet export
name=sve-export-$(date -u +%Y%m%dT%H%M%SZ)
args=(snapshot export-offline --inputs "$run_dir/recipe.json" --bundle-dir "$run_dir/bundle"
  --preview-dir "$SVE_EXPORT_DIR" --private-dir "$SVE_CARDDB_PRIVATE_DIR" --workers "${EXPORT_WORKERS:-2}")
if [[ ${EXPORT_IMAGES:-1} == 1 ]]; then
  args+=(--image-assets-dir "$SVE_IMAGE_ASSETS_DIR" --image-cache-dir "$SVE_IMAGE_CACHE_DIR")
fi
if [[ ${EXPORT_BROTLI:-1} == 1 ]]; then args+=(--brotli); fi
entry=()
if [[ ${EXPORT_PROFILE:-0} == 1 ]]; then
  # Collapsed stacks open in speedscope and are easy to rank; SIGINT makes py-spy write them.
  entry=(--entrypoint py-spy)
  args=(record --format raw --rate 20 -o "$run_dir/profile.txt" -- python -c "from sve_carddb.cli import main; main()" "${args[@]}")
fi
"${compose[@]}" run -d --name "$name" "${entry[@]}" export "${args[@]}" "$@" >/dev/null
docker logs -f "$name" >"$run_dir/export.log" 2>&1 &
logs=$!
echo "started $name; logs in $run_dir"

# cgroup v2 lists one unified path per process, wherever the Docker driver placed the container.
pid=$(docker inspect -f '{{.State.Pid}}' "$name")
cgroup_path=$(sed -n 's/^0:://p' "/proc/$pid/cgroup" 2>/dev/null || true)
cgroup=${cgroup_path:+/sys/fs/cgroup$cgroup_path}
[[ -n $cgroup ]] || echo "cgroup of $name not found; resources.tsv stays empty" >&2

# Recorded only: the caps already bound the export, and pressure here usually comes from other work.
psi_file=/sys/fs/cgroup/user.slice/user-$HOST_UID.slice/user@$HOST_UID.service/memory.pressure
[[ -r $psi_file ]] || psi_file=/proc/pressure/memory

started=$(date +%s)
tsv=$run_dir/resources.tsv
printf 'utc\telapsed_s\tcpu_pct\tmem_gib\tanon_gib\tfile_gib\tpeak_gib\thost_avail_gib\tuser_psi_avg10\n' >"$tsv"
peak='' anon_peak=0 cpu_total='' last_cpu='' last_wall=$started tick=0
while [[ $(docker inspect -f '{{.State.Running}}' "$name" 2>/dev/null) == true ]]; do
  # The cgroup disappears the moment the container exits, so every read may come back empty.
  mem='' anon='' file=''
  if [[ -n $cgroup ]]; then
    mem=$(cat "$cgroup/memory.current" 2>/dev/null || true)
    # memory.current counts reclaimable page cache too; anon is what the export itself must keep.
    read -r anon file < <(awk '$1 == "anon" {a = $2} $1 == "file" {f = $2} END {print a, f}' "$cgroup/memory.stat" 2>/dev/null || true) || true
    if [[ -n $anon ]] && ((anon > anon_peak)); then anon_peak=$anon; fi
    value=$(cat "$cgroup/memory.peak" 2>/dev/null || true)
    if [[ -n $value ]]; then peak=$value; fi
    value=$(awk '/^usage_usec/ {print $2}' "$cgroup/cpu.stat" 2>/dev/null || true)
    if [[ -n $value ]]; then cpu_total=$value; fi
  fi
  psi=$(awk '/^some/ {split($2, a, "="); print a[2]}' "$psi_file")
  if ((tick % 3 == 0)) && [[ -n $mem ]]; then
    now=$(date +%s)
    awk -v utc="$(date -u +%FT%TZ)" -v el=$((now - started)) -v cpu="$cpu_total" -v last="$last_cpu" \
      -v dt=$((now - last_wall)) -v mem="$mem" -v anon="$anon" -v file="$file" -v pk="$peak" -v psi="$psi" \
      -v av="$(awk '/^MemAvailable:/ {print $2}' /proc/meminfo)" \
      'function gib(v) { return (v == "") ? "n/a" : sprintf("%.1f", v / 2^30) }
      BEGIN {
        pct = (last == "" || dt <= 0) ? "n/a" : sprintf("%d", (cpu - last) / (dt * 10000))
        printf "%s\t%d\t%s\t%s\t%s\t%s\t%s\t%.1f\t%s\n", utc, el, pct, gib(mem), gib(anon), gib(file), gib(pk), av / 2^20, psi
      }' >>"$tsv"
    last_cpu=$cpu_total last_wall=$now
  fi
  tick=$((tick + 1))
  sleep 10
done

# docker wait still yields the container's own exit code when the loop ended early.
rc=$(docker wait "$name")
wait "$logs" || true
oom=$(docker inspect -f '{{.State.OOMKilled}}' "$name")
awk -v rc="$rc" -v oom="$oom" -v el=$(($(date +%s) - started)) -v cpu="$cpu_total" -v pk="$peak" -v ap="$anon_peak" \
  'BEGIN {
    printf "exit=%s oom_killed=%s elapsed=%dm%02ds cpu=%s peak_mem=%s sampled_anon_peak=%.1fGiB\n", rc, oom,
      el / 60, el % 60, (cpu == "") ? "n/a" : sprintf("%dm", cpu / 6e7),
      (pk == "") ? "n/a" : sprintf("%.1fGiB", pk / 2^30), ap / 2^30
  }' | tee "$run_dir/summary.txt"
docker rm "$name" >/dev/null
exit "$rc"
