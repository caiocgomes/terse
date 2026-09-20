# raw-tex fixtures

Original, hand-authored fixtures (no third-party provenance) exercising the
`tex:` raw escape hatch introduced in task group 8. `drawing.trs` declares a
local TikZ drawing using the built-in `tikz` package
(`[latex] packages = ["tikz"]` in the accompanying `terse.toml`), which
`test_declared_tikz_support_compiles` (task 8.6, `#[ignore]`d without a local
XeLaTeX/TikZ install) compiles end-to-end.
