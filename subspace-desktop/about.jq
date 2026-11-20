# Cargo's normal dependency tree omits compile-time proc macros. Use names and
# versions to match it against cargo-about's resolved licence records.
def key: "\(.name) v\(.version)";

(
  $tree | split("\n") | map(select(length > 0) |
    capture("^(?<name>\\S+) v(?<version>\\S+)") | key
  ) | unique
) as $linked
| ([.crates[].package | key] | unique) as $known
| if ($linked | length) == 0 then error("empty release dependency tree")
  elif ($linked - $known | length) > 0 then error("missing licence records for " + ($linked - $known | join(", ")))
  else . end
| .licenses |= map(
    .used_by |= map(select((.crate | key) as $crate | $linked | index($crate)))
    | select(.used_by | length > 0)
  )
| ([.licenses[].used_by[].crate | key] | unique) as $covered
| if ($linked - $covered | length) > 0 then error("missing licence texts for " + ($linked - $covered | join(", ")))
  else . end
| "# Third-party Rust crate licences",
  "",
  "This report covers the Subspace binary for \($target). See NOTICE for other bundled components.",
  "",
  (
    .licenses | group_by(.text) | sort_by(.[0].id)[] |
    "## \(map(.id) | unique | join(", "))",
    "",
    (map(.used_by[].crate | key) | unique[] | "- \(.)"),
    "",
    "~~~~",
    .[0].text,
    "~~~~",
    ""
  )
