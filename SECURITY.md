# Security Policy

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it
privately through GitHub's vulnerability reporting: on the repository's
**Security** tab, choose **Report a vulnerability**. Include the Terse
version or commit, your operating system, and the smallest input (`.trs`
file, theme, manifest, or command line) that reproduces the problem.

You should get an acknowledgement within a week. Once a fix is ready, it is
released and the advisory is published with credit to the reporter, unless
you ask not to be named.

## Supported versions

Terse is pre-1.0. Only the latest commit on `main` and the most recent
release receive security fixes.

## Scope

Terse compiles documents that may come from someone else, so the boundaries
it claims are worth testing. In scope:

- Restricted math. `math:` blocks, `\(...\)`, and `$...$` are checked
  against a closed allowlist, and execution primitives (`\input`,
  `\write18`, `\csname`, `\def`, `^^` byte encoding, and similar) are
  rejected at parse time. Any way to get such a primitive through to the
  generated LaTeX is a vulnerability.
- Path confinement. Includes, assets, theme files, and output paths are
  resolved inside the project, and `..` or a symlink that leads past the
  project root is an error.
  Any way to read or write outside the project is a vulnerability.
- Engine invocation. XeLaTeX is always started with `-no-shell-escape`.
- Network and toolchain. Only `terse refs resolve` and
  `terse toolchain install|update` use the network. The managed TeX Live
  installer is checked against the SHA-512 pinned in
  `crates/terse-core/profiles/toolchain-texlive-2025.toml`. A way to make
  another command reach the network, or to install an installer that does
  not match the pin, is a vulnerability.

Out of scope:

- Raw TeX in `tex:` blocks. It is passed to the engine as written, by
  design, and always raises warning `W-TEX-001`; use
  `terse check --deny-warnings` to refuse documents that contain it.
- Vulnerabilities in TeX Live, XeLaTeX, or Biber themselves. Report those
  upstream.
