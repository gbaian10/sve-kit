(module
  (memory (export "mem") 80)
  ;; out[i] = max(dot(row_i, e0), dot(row_i, e1)); rows int8 [n,256] at $q, e0/e1 int16 [2,256] at $e
  (func (export "dots") (param $n i32) (param $q i32) (param $e i32) (param $out i32)
    (local $i i32) (local $j i32) (local $p i32) (local $a0 v128) (local $a1 v128)
    (local $v v128) (local $lo v128) (local $hi v128) (local $ej i32) (local $s0 i32) (local $s1 i32)
    (block $done (loop $rows
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $a0 (v128.const i32x4 0 0 0 0)) (local.set $a1 (v128.const i32x4 0 0 0 0))
      (local.set $p (i32.add (local.get $q) (i32.shl (local.get $i) (i32.const 8))))
      (local.set $j (i32.const 0))
      (loop $cols
        (local.set $v (v128.load (i32.add (local.get $p) (local.get $j))))
        (local.set $lo (i16x8.extend_low_i8x16_s (local.get $v)))
        (local.set $hi (i16x8.extend_high_i8x16_s (local.get $v)))
        (local.set $ej (i32.add (local.get $e) (i32.shl (local.get $j) (i32.const 1))))
        (local.set $a0 (i32x4.add (local.get $a0) (i32x4.add
          (i32x4.dot_i16x8_s (local.get $lo) (v128.load (local.get $ej)))
          (i32x4.dot_i16x8_s (local.get $hi) (v128.load offset=16 (local.get $ej))))))
        (local.set $a1 (i32x4.add (local.get $a1) (i32x4.add
          (i32x4.dot_i16x8_s (local.get $lo) (v128.load offset=512 (local.get $ej)))
          (i32x4.dot_i16x8_s (local.get $hi) (v128.load offset=528 (local.get $ej))))))
        (local.set $j (i32.add (local.get $j) (i32.const 16)))
        (br_if $cols (i32.lt_u (local.get $j) (i32.const 256))))
      (local.set $s0 (i32.add (i32.add (i32x4.extract_lane 0 (local.get $a0)) (i32x4.extract_lane 1 (local.get $a0)))
                              (i32.add (i32x4.extract_lane 2 (local.get $a0)) (i32x4.extract_lane 3 (local.get $a0)))))
      (local.set $s1 (i32.add (i32.add (i32x4.extract_lane 0 (local.get $a1)) (i32x4.extract_lane 1 (local.get $a1)))
                              (i32.add (i32x4.extract_lane 2 (local.get $a1)) (i32x4.extract_lane 3 (local.get $a1)))))
      (i32.store (i32.add (local.get $out) (i32.shl (local.get $i) (i32.const 2)))
        (select (local.get $s0) (local.get $s1) (i32.gt_s (local.get $s0) (local.get $s1))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $rows))))
)
