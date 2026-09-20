# Provider fixtures

Hand-authored CSL-JSON and Atom XML payloads shaped after the public,
documented Crossref/DOI content-negotiation (CSL-JSON) and arXiv `id_list`
(Atom) response formats. None of these were captured from live traffic —
they are minimal literal examples adapted from each provider's published
schema documentation, used to test group 14's normalization/adapter code
entirely offline.

- `doi-match.json` — a CSL-JSON record whose `DOI` field matches the
  identifier that would have been requested.
- `doi-mismatch.json` — a CSL-JSON record whose `DOI` field does not match
  the requested identifier (identity-check rejection case).
- `doi-malformed.json` — truncated/invalid JSON (parse-failure case).
- `arxiv-entry.atom` — one arXiv `id_list` Atom feed entry with an explicit
  version (`v2`) in its `<id>`, used for both versionless-pinning and
  explicit-version-verification cases.
- `arxiv-empty.atom` — a feed with no `<entry>`, as arXiv returns for an
  unknown id (no HTTP error).
- `arxiv-dtd.atom` — the same entry, but with a `<!DOCTYPE>`/external
  entity declaration prepended, to document that `quick-xml`'s pull parser
  never resolves DTDs or external entities regardless of input (XXE is
  structurally impossible here, not merely disabled by configuration).
