# Licensing

The project's own software uses Apache-2.0. Our own data contributions use
CC0-1.0, including our contributions to translations. Official and other
third-party content is excluded. The repository as a whole is not a single
freely licensed work.

## Path-based scope

Specific paths take precedence over broader directories. The README row applies
to README files even inside a data or official-assets directory. Other original
project documentation uses Apache-2.0 as shown below.

| Path | License for our contributions | Excluded third-party material |
| --- | --- | --- |
| `carddb/`, `sim/`, `dsl/`, `.github/`, and `tests/`, except the more specific rows below | Apache-2.0 | Official names, card wording, rules, Q&A, source-derived text and other third-party material in literals, examples or fixtures |
| `authored/effects/`, `authored/keywords.yaml`, `authored/wording-rules/` | Apache-2.0 | Official names in `name` and `ja`, copied wording, and other official or third-party material |
| All other paths under `authored/`, including translations, rulings, registry, IDs, products, catalog adoptions, and their indexes | CC0-1.0 | `evidence[].quote`; official or third-party translations, including values with `origin: official_*`; `expected_raw_value` and `corrected_value` containing source expression; official product names in `authored/products/` `text` values; official vocabulary in `authored/catalog-adoptions/` `raw` values; other identified third-party material |
| `docs/`, README files at any level, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, and project-owned legal documentation | Apache-2.0 | Real-card examples, official excerpts and source quotations, including those in `docs/dsl/` and `docs/m0/` |
| Other repository-root configuration files, including dotfiles and lockfiles | Apache-2.0 | Third-party notices and material retain their own terms |
| `tests/rules-scenarios/questions/` | Apache-2.0 | `evidence[].quote` and any other official or third-party source wording are quotations, not licensed by the project |
| `sim/web/src/assets/official/` image files, including `logo/head-left.png` | No project license | All official images, including cropped derivatives; the directory README follows the documentation row |
| `LICENSE`, `LICENSES/` | The supplied standard texts and exclusion notice identify their own terms and scope | These texts do not override the exclusions above |

The license in the middle column applies to our contributions at that path.
You do not need to classify our individual fields as code, facts or prose to
choose a project license. The right-hand column identifies material we cannot
license. A provenance tag or review receipt does not establish ownership.
New top-level paths outside the categories above must be assigned a scope before
acceptance; unlisted material is not implicitly granted a license by this table.

`LICENSES/Apache-2.0.txt` is a relative symbolic link (`../LICENSE`) to the
root `LICENSE` file, which contains the Apache-2.0 license text.

## Translations and data

For our own contributions in the CC0 paths, we apply CC0 to the fullest extent
of rights we hold and can dispose of. We do not require attribution to us as a
condition of CC0. Source credits remain useful provenance and must not be
mistaken for permission from source-work owners.

This includes our own contributions to translations. We do not claim rights
against reuse of those contributions; CC0 provides its waiver and fallback
within its terms. This does not dedicate official wording, official names,
official translations, community translations, or underlying source works to
the public domain. Whether a complete translation may be reused depends on the
rights in its source work and applicable law. AI generation and our acceptance
do not clear those rights.

## Retained quotations and official assets

The finalized questions under `tests/rules-scenarios/questions/` and rulings
under `authored/rulings/` contain source quotations used to explain expected
outcomes. Their source references are retained. The project licenses its own
contributions under the path table, not the quoted material. Permission to
publish these files as a project decision is not a sublicense from the source
rights holders.

Official content in documents and test programs is likewise excluded. In
particular, real-card examples and quotations in `docs/dsl/` and quoted fragments
in `docs/m0/` are not covered by the project's Apache grant. Synthetic test
content follows its path's project license. Any official or other third-party
source expression embedded in test programs remains excluded from that grant.

The current tree still contains source-derived fixtures and real-card assertions
under `carddb/tests/`; their replacement with synthetic data is tracked in
[issue #228](https://github.com/gbaian10/sve-kit/issues/228). Duplicate question
originals under `tests/rules-scenarios/originals/` also remain in the current tree,
with removal planned separately. Their official quotations and source-derived
material are excluded from the project grants while present, as well as in
historical revisions after removal.

The site image `sim/web/src/assets/official/logo/head-left.png` is a crop of an
official character illustration, currently used as the site's logo. It is not an
original sve-kit logo. It remains excluded, along with the 7 class icons and 32
card-text symbols. Its use as site branding does not establish endorsement or
permission to redistribute it in favicons, PWA icons, or installers. See the
assets README for source and modification details.

sve-kit is unofficial and is not affiliated with or endorsed by Cygames,
Bushiroad, or other game rights holders. Official material remains subject to
the rights of its respective owners and licensors. No official trademark rights
are granted. These exclusions do not restrict uses independently allowed by
law or by the relevant rights holder.

If a file combines project contributions and excluded official content, a
component description may use `Apache-2.0 AND LicenseRef-sve-kit-Official-Content`
or `CC0-1.0 AND LicenseRef-sve-kit-Official-Content`, according to its path.
AND records the applicable component terms together; it does not relicense the
official portion. The LicenseRef is an explicit no-grant notice, not an open-source
license. There is no alternative-license choice for the complete mixed file.

## Previous revisions

The repository retains its Git history. When question originals are deleted or
source-derived test fixtures are replaced, their earlier versions remain
accessible in previous revisions. Official and other third-party content there is excluded
from the project's grants just as it is in the current tree. A license added to
a later revision does not relicense earlier third-party material.

Keeping historical copies is the maintainer's repository policy, not a grant of
rights from their owners. Security credentials discovered in a history scan are
handled separately, with revocation or rotation first.

## AI production and ownership

1. Almost all project code and data were generated by AI models under the
   maintainer's direction.
2. Copyright protection depends on human creative contributions and applicable
   law. The project grants only rights it holds; these licenses do not create
   copyright or grant rights in official or other third-party content.
3. `Copyright 2026 gbaian10` covers the maintainer's rights to the extent they
   exist; other contributors retain theirs. Author and review records describe
   participation, not proof of legal authorship or permission to use source works.

## Dependencies and published content

Third-party software, fonts, and general-purpose icons retain their own terms.
Distributions must carry the notices required for components actually included.
In particular, Lucide includes ISC and Feather MIT notices, and IBM Plex Sans
uses OFL-1.1. Root NOTICE is not a substitute for a release's dependency notices.

Snapshots, images and audio do not acquire the license of the program that
exports them. They may include official and third-party content; no project
license is granted over a mixed snapshot as a whole. Published content must be
accompanied by the website's legal information and a human-readable CDN notice.
No blanket `license` field in a snapshot is inferred from this document.

## Rights contact

For rights concerns or removal requests, contact `contact@svekit.app`.
Include the URL or file/version, the rights concerned, your authority to act,
the requested action, and a reply address. Do not publish private identification
documents in an issue. The maintainer will review the request; this process is
not a claim of statutory safe-harbor registration or official permission.
