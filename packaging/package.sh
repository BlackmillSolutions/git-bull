#!/usr/bin/env bash
# Packs the release packages of one platform from a release build (spec
# `distribution`, task 9.2). Each package holds both licence texts, the
# third-party notices and the README.
#
# Usage: packaging/package.sh <kind> <version> <binary> <out-dir>
#   kind     windows-x64, macos-arm64, macos-x64 or linux-x64
#   version  such as 0.1.0 or 0.1.0-rc.1
#   binary   the release build of git-bull for that platform
#   out-dir  where the packages go; created when missing
#
# linux-x64 makes the tar.gz archive and the AppImage; the latter needs
# APPIMAGETOOL and APPIMAGE_RUNTIME, the paths of appimagetool and of the
# AppImage runtime.
set -euo pipefail

kind=$1
version=$2
binary=$3
mkdir -p "$4"
out=$(cd "$4" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
docs=("$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/THIRD-PARTY-NOTICES.md" "$root/README.md")
name="git-bull-$version-$kind"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

case "$kind" in
windows-x64)
    mkdir "$stage/$name"
    cp "$binary" "${docs[@]}" "$stage/$name/"
    (cd "$stage" && 7z a -tzip -bso0 -bsp0 "$out/$name.zip" "$name")
    ;;
macos-arm64 | macos-x64)
    app="$stage/$name/git-bull.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$app/Contents/MacOS/git-bull"
    # A bundle version is numbers only, without a pre-release suffix.
    sed "s/@VERSION@/${version%%-*}/g" "$root/packaging/macos/Info.plist" >"$app/Contents/Info.plist"
    cp "${docs[@]}" "$app/Contents/Resources/"
    cp "${docs[@]}" "$stage/$name/"
    # An ad-hoc signature, not one with a developer certificate: macOS on
    # Apple silicon runs no code without any signature.
    codesign --force --deep --sign - "$app"
    (cd "$stage" && ditto -c -k --keepParent "$name" "$out/$name.zip")
    ;;
linux-x64)
    mkdir "$stage/$name"
    cp "$binary" "${docs[@]}" "$stage/$name/"
    tar -C "$stage" -czf "$out/$name.tar.gz" "$name"

    appdir="$stage/AppDir"
    mkdir -p "$appdir/usr/bin" "$appdir/usr/share/doc/git-bull" \
        "$appdir/usr/share/applications" "$appdir/usr/share/icons/hicolor/scalable/apps"
    cp "$binary" "$appdir/usr/bin/git-bull"
    cp "${docs[@]}" "$appdir/usr/share/doc/git-bull/"
    cp "$root/packaging/linux/git-bull.desktop" "$appdir/"
    cp "$root/packaging/linux/git-bull.desktop" "$appdir/usr/share/applications/"
    cp "$root/packaging/git-bull.svg" "$appdir/"
    cp "$root/packaging/git-bull.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/"
    cat >"$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/git-bull" "$@"
EOF
    chmod +x "$appdir/AppRun" "$appdir/usr/bin/git-bull"
    # The runtime is given, so that appimagetool downloads nothing; it runs
    # extracted, as the build machine may have no FUSE.
    ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" --no-appstream \
        --runtime-file "$APPIMAGE_RUNTIME" "$appdir" "$out/$name.AppImage"
    ;;
*)
    echo "unknown kind: $kind" >&2
    exit 1
    ;;
esac
