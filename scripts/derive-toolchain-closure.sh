#!/usr/bin/env bash
# Derives the `[closure].derived` list of crates/terse-core/profiles/
# toolchain-texlive-2025.toml from a real compile, so the pinned closure can
# never silently drift from what terse-style.sty requires.
#
# Steps: install TeX Live `scheme-infraonly` from the frozen repository into a
# scratch prefix (portable mode), install the current closure with tlmgr,
# generate tests/fixtures/full-paper under both themes (plus a pt-BR variant),
# compile each with `-recorder`, and map every INPUT under texmf-dist to its
# owning package through the repository's texlive.tlpdb. Prints the sorted
# owner set and every input it could not map. Needs: perl, curl or wget, xz,
# a built `terse` binary (TERSE=... or target/debug/terse), and network access
# to the repository. Maintainer tooling; not part of the test lanes.
set -euo pipefail

REPO="${REPO:-https://ftp.math.utah.edu/pub/tex/historic/systems/texlive/2025/tlnet-final/}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${WORK:-$(mktemp -d "${TMPDIR:-/tmp}/terse-closure.XXXXXX")}"
TERSE="${TERSE:-$ROOT/target/debug/terse}"
PROFILE_TOML="$ROOT/crates/terse-core/profiles/toolchain-texlive-2025.toml"

for tool in perl xz tar; do command -v "$tool" >/dev/null || { echo "missing prerequisite: $tool" >&2; exit 3; }; done
command -v curl >/dev/null || command -v wget >/dev/null || { echo "missing prerequisite: curl or wget" >&2; exit 3; }
[ -x "$TERSE" ] || { echo "terse binary not found at $TERSE (set TERSE=...)" >&2; exit 3; }

fetch() { if command -v curl >/dev/null; then curl -fsSL --max-time 600 -o "$2" "$1"; else wget -q -O "$2" "$1"; fi; }

TEXDIR="$WORK/texlive"
mkdir -p "$TEXDIR"
echo "work dir: $WORK" >&2

# 1. Installer from the frozen repository, checksum verified against the pin.
fetch "${REPO}install-tl-unx.tar.gz" "$WORK/install-tl-unx.tar.gz"
PIN=$(awk -F'"' '/^sha512 = /{print $2}' "$PROFILE_TOML")
ACTUAL=$(shasum -a 512 "$WORK/install-tl-unx.tar.gz" | awk '{print $1}')
[ "$PIN" = "$ACTUAL" ] || { echo "install-tl checksum mismatch: pinned $PIN, got $ACTUAL" >&2; exit 3; }
tar -xzf "$WORK/install-tl-unx.tar.gz" -C "$WORK"
INSTALLER=$(ls -d "$WORK"/install-tl-*/ | head -1)

cat > "$WORK/install.profile" <<EOF
selected_scheme scheme-infraonly
TEXDIR $TEXDIR
TEXMFSYSCONFIG $TEXDIR/texmf-config
TEXMFSYSVAR $TEXDIR/texmf-var
TEXMFLOCAL $TEXDIR/texmf-local
TEXMFHOME $WORK/texmf-home
TEXMFVAR $WORK/texmf-var
TEXMFCONFIG $WORK/texmf-config
instopt_portable 1
instopt_adjustpath 0
instopt_letter 0
tlpdbopt_install_docfiles 0
tlpdbopt_install_srcfiles 0
tlpdbopt_autobackup 0
tlpdbopt_desktop_integration 0
tlpdbopt_file_assocs 0
tlpdbopt_post_code 1
EOF

CLEAN_ENV=(env -i PATH="/usr/bin:/bin:/usr/sbin:/sbin:$(dirname "$(command -v perl)"):$(dirname "$(command -v xz)")" HOME="$HOME" TMPDIR="${TMPDIR:-/tmp}" LANG=C.UTF-8)
"${CLEAN_ENV[@]}" perl "$INSTALLER/install-tl" -no-gui -profile "$WORK/install.profile" -repository "$REPO" > "$WORK/install-tl.log" 2>&1
BIN=$(ls -d "$TEXDIR"/bin/*/ | head -1); BIN="${BIN%/}"

# 2. Current closure (derived + manual) so the compile has a chance to finish.
PKGS=$(awk '/^derived = \[/,/^\]/{print} /^manual = \[/,/^\]/{print}' "$PROFILE_TOML" | grep -o '"[^"]*"' | tr -d '"' | tr '\n' ' ')
"${CLEAN_ENV[@]}" PATH="$BIN:/usr/bin:/bin:/usr/sbin:/sbin" tlmgr --repository "$REPO" install $PKGS > "$WORK/tlmgr.log" 2>&1

# 3. Full catalog for ownership mapping (RELOC/ prefix means texmf-dist/).
fetch "${REPO}tlpkg/texlive.tlpdb.xz" "$WORK/texlive.tlpdb.xz"
xz -d "$WORK/texlive.tlpdb.xz"

# 4. Generate and compile the fixture variants with -recorder.
compile() {
    local dir="$1"
    ( cd "$dir" && "${CLEAN_ENV[@]}" PATH="$BIN:/usr/bin:/bin" TEXMFHOME="$WORK/texmf-home" TEXMFVAR="$WORK/texmf-var" TEXMFCONFIG="$WORK/texmf-config" \
        sh -c "xelatex -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error -recorder paper.tex >x1.log 2>&1 && biber paper >b.log 2>&1 && xelatex -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error -recorder paper.tex >x2.log 2>&1 && xelatex -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error -recorder paper.tex >x3.log 2>&1" ) \
        || { echo "compile failed in $dir (see x1.log/b.log)" >&2; grep -m3 '^!' "$dir/x3.log" "$dir/x1.log" >&2 || true; exit 1; }
}
FLS=()
for variant in en pt-BR; do
    P="$WORK/fixture-$variant"
    rm -rf "$P"; cp -R "$ROOT/tests/fixtures/full-paper" "$P"; rm -rf "$P/build" "$P/.terse-cache"
    if [ "$variant" = "pt-BR" ]; then sed -i.bak 's/^  language: .*/  language: pt-BR/' "$P/paper.trs" && rm -f "$P/paper.trs.bak"; fi
    for theme in academic magalu; do
        ( cd "$P" && "$TERSE" build --tex-only --theme "$theme" >/dev/null )
        compile "$P/build/$theme"
        FLS+=("$P/build/$theme/paper.fls")
    done
done

# 5. Map INPUT files to owning packages.
awk -v dist="$(cd "$TEXDIR/texmf-dist" && pwd -P)" '
    FNR == 1 { mode = (FILENAME == ARGV[1]) ? "tlpdb" : "fls"; dir = FILENAME; sub(/\/[^\/]*$/, "", dir) }
    mode == "tlpdb" {
        if ($0 ~ /^name /) { name = $2; files = 0; next }
        if ($0 ~ /^(runfiles|binfiles) /) { files = 1; next }
        if ($0 ~ /^(docfiles|srcfiles) /) { files = 0; next }
        if ($0 ~ /^ / && files) { rel = $1; sub(/^ +/, "", rel); sub(/^RELOC\//, "texmf-dist/", rel); owner[rel] = name; next }
        if ($0 == "") { files = 0 }
        next
    }
    /^INPUT / {
        p = substr($0, 7)
        if (p !~ /^\//) { p = dir "/" p }
        gsub(/\/\.\//, "/", p)
        if (index(p, dist "/") != 1) { outside[p] = 1; next }
        rel = "texmf-dist/" substr(p, length(dist) + 2)
        if (rel in owner) { pkgs[owner[rel]] = 1 } else { unmapped[rel] = 1 }
    }
    END {
        for (p in pkgs) print "DERIVED\t" p
        for (u in unmapped) print "UNMAPPED\t" u
    }' "$WORK/texlive.tlpdb" "${FLS[@]}" | LC_ALL=C sort > "$WORK/owners.tsv"

echo "derived = ["
awk -F'\t' '$1=="DERIVED"{print "    \"" $2 "\","}' "$WORK/owners.tsv"
echo "]"
echo "# unmapped inputs under texmf-dist: $(grep -c '^UNMAPPED' "$WORK/owners.tsv" || true)"
awk -F'\t' '$1=="UNMAPPED"{print "# " $2}' "$WORK/owners.tsv"
echo "work dir kept at $WORK" >&2
