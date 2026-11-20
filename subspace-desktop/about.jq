# The resolver matches Cargo's normal tree to package IDs including sources.
def crate_label: "\(.name) v\(.version)";

.linked as $linked
| .licenses |= map(
    .used_by |= map(select(.crate.id as $crate | $linked | index($crate)))
    | select(.used_by | length > 0)
  )
| ([.licenses[].used_by[].crate.id] | unique) as $covered
| if ($linked - $covered | length) > 0 then error("missing licence texts for " + ($linked - $covered | join(", ")))
  else . end
| ([.licenses[] | select(has("verified_crates")) |
    .id as $id | .verified_crates as $verified | .used_by[].crate |
    select(.id as $crate | $verified | index($crate) == null) |
    "\(crate_label) (\($id), \(.id))"
  ] | unique) as $unexplained
| if ($unexplained | length) > 0 then
    error("missing cargo-about fallback diagnostics for " + ($unexplained | join(", ")))
  else . end
| "# Third-party Rust crate licences",
  "",
  "This report covers the Subspace binary for \($target). See NOTICE for other bundled components.",
  "The Rust standard library copyright and licence notices are in licenses/rust-std/.",
  "",
  (
    .fallbacks | map(select(.crate.id as $crate | $linked | index($crate))) |
    if length > 0 then
      "## Generic SPDX texts",
      "",
      "Cargo-about could not find a source-backed licence text for these entries.",
      "",
      (.[] | "- \(.crate | crate_label) (\(.id))"),
      ""
    else empty end
  ),
  (
    [.licenses[] | select(.id == "MPL-2.0") | .used_by[].crate] | unique_by(.id) |
    if length > 0 then
      "## MPL-2.0 source code",
      "",
      "The corresponding source is available under the MPL-2.0.",
      "",
      (.[] | if .source == "registry+https://github.com/rust-lang/crates.io-index" then
        "- \(crate_label) <https://static.crates.io/crates/\(.name)/\(.name)-\(.version).crate>"
      else error("missing MPL-2.0 source URL for " + .id) end),
      ""
    else empty end
  ),
  (
    .licenses | group_by(.text) | sort_by(.[0].id)[] |
    "## \(map(.id) | unique | join(", "))",
    "",
    (map(.used_by[].crate | crate_label) | unique[] | "- \(.)"),
    "",
    "~~~~",
    .[0].text,
    "~~~~",
    ""
  )
