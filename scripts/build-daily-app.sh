#!/bin/sh
# Build a fresh, identifiable local Chartr Daily bundle. This does not install it.
set -eu

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
repo_root=$(cd "$script_dir/.." && pwd)

if [ "$#" -ne 0 ]; then
    echo "usage: $0" >&2
    exit 2
fi

source_revision=$(git -C "$repo_root" rev-parse HEAD)
source_short=$(git -C "$repo_root" rev-parse --short=12 HEAD)
source_branch=$(git -C "$repo_root" branch --show-current)
source_state=clean
if [ -n "$(git -C "$repo_root" status --porcelain)" ]; then
    source_state=dirty
fi

echo "Building Chartr Daily from $source_branch at $source_revision ($source_state)"
cargo build --manifest-path "$repo_root/Cargo.toml" \
    --offline --locked -p chartr --bin chartr \
    --features gpui_platform/runtime_shaders

main_binary="$repo_root/target/debug/chartr"
sidecar_binary="$repo_root/target/debug/herdr"
if [ ! -x "$main_binary" ] || [ ! -x "$sidecar_binary" ]; then
    echo "build did not produce executable chartr and herdr binaries" >&2
    exit 1
fi

expected_herdr=$(sed -n \
    's/^pub const SUPPORTED_HERDR_VERSION: &str = "\(.*\)";$/\1/p' \
    "$repo_root/crates/chartr-herdr/src/lib.rs")
actual_herdr=$("$sidecar_binary" --version | sed 's/^herdr //')
if [ -z "$expected_herdr" ] || [ "$actual_herdr" != "$expected_herdr" ]; then
    echo "Herdr sidecar is $actual_herdr; expected $expected_herdr" >&2
    exit 1
fi

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repo_root/Cargo.toml" | head -1)
if [ -z "$version" ]; then
    echo "cannot read the workspace version from Cargo.toml" >&2
    exit 1
fi

mkdir -p "$repo_root/target"
stage_dir=$(mktemp -d "$repo_root/target/daily-app.XXXXXX")
app="$stage_dir/Chartr Daily.app"
macos="$app/Contents/MacOS"
resources="$app/Contents/Resources"
mkdir -p "$macos" "$resources"
ditto "$main_binary" "$macos/chartr"
ditto "$sidecar_binary" "$macos/herdr"

iconset="$stage_dir/chartr.iconset"
mkdir -p "$iconset"
for spec in \
    "16 icon_16x16 16" \
    "32 icon_16x16@2x 32" \
    "32 icon_32x32 32" \
    "64 icon_32x32@2x 1024" \
    "128 icon_128x128 1024" \
    "256 icon_128x128@2x 1024" \
    "256 icon_256x256 1024" \
    "512 icon_256x256@2x 1024" \
    "512 icon_512x512 1024" \
    "1024 icon_512x512@2x 1024"
do
    set -- $spec
    sips -z "$1" "$1" "$repo_root/docs/assets/v4/icon-mac-$3.png" \
        --out "$iconset/$2.png" >/dev/null
done
icon_name=chartr.icns
if ! iconutil -c icns "$iconset" -o "$resources/$icon_name"; then
    # Some Command Line Tools installations cannot compile an iconset. A PNG
    # resource still gives the local bundle an icon without blocking the build.
    icon_name=chartr.png
    ditto "$repo_root/docs/assets/v4/icon-mac-1024.png" "$resources/$icon_name"
    echo "warning: iconutil failed; bundled the source PNG icon instead" >&2
fi

bundle_id=dev.chartr.daily-ui
plist="$app/Contents/Info.plist"
plutil -create xml1 "$plist"
plutil -insert CFBundleDevelopmentRegion -string en "$plist"
plutil -insert CFBundleDisplayName -string 'Chartr Daily' "$plist"
plutil -insert CFBundleExecutable -string chartr "$plist"
plutil -insert CFBundleIconFile -string "$icon_name" "$plist"
plutil -insert CFBundleIdentifier -string "$bundle_id" "$plist"
plutil -insert CFBundleInfoDictionaryVersion -string 6.0 "$plist"
plutil -insert CFBundleName -string 'Chartr Daily' "$plist"
plutil -insert CFBundlePackageType -string APPL "$plist"
plutil -insert CFBundleShortVersionString -string "${version%%-*}" "$plist"
plutil -insert chartrReleaseVersion -string "$version" "$plist"
plutil -insert chartrGitRevision -string "$source_revision" "$plist"
plutil -insert chartrSourceTree -string "$source_state" "$plist"
plutil -insert LSApplicationCategoryType -string public.app-category.developer-tools "$plist"
plutil -insert NSHighResolutionCapable -bool YES "$plist"
plutil -insert NSQuitAlwaysKeepsWindows -bool NO "$plist"
plutil -insert NSDisablePersistentUI -bool YES "$plist"
minimum_macos=$(otool -l "$main_binary" | awk '$1 == "minos" { print $2; exit }')
if [ -z "$minimum_macos" ]; then
    minimum_macos=11.0
fi
plutil -insert LSMinimumSystemVersion -string "$minimum_macos" "$plist"
plutil -lint "$plist"

# A stable identity keeps macOS privacy grants (such as Documents access)
# across rebuilds; ad-hoc signatures look like a new app every time.
sign_identity=${CHARTR_SIGN_IDENTITY:-Chartr Daily Dev}
if security find-identity -p codesigning | grep -Fq "\"$sign_identity\""; then
    echo "signing with \"$sign_identity\""
else
    echo "warning: no \"$sign_identity\" code-signing identity; signing ad-hoc" >&2
    sign_identity=-
fi
codesign --force --sign "$sign_identity" --timestamp=none \
    --identifier "$bundle_id.herdr" "$macos/herdr"
codesign --force --sign "$sign_identity" --timestamp=none \
    --identifier "$bundle_id" "$macos/chartr"
codesign --force --sign "$sign_identity" --timestamp=none \
    --identifier "$bundle_id" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

actual_bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")
actual_revision=$(/usr/libexec/PlistBuddy -c 'Print :chartrGitRevision' "$plist")
if [ "$actual_bundle_id" != "$bundle_id" ] || [ "$actual_revision" != "$source_revision" ]; then
    echo "staged bundle identity or revision mismatch" >&2
    exit 1
fi

echo "STAGED_APP=$app"
echo "SOURCE_REVISION=$source_revision"
echo "SOURCE_TREE=$source_state"
shasum -a 256 "$macos/chartr" "$macos/herdr"
echo "The installed Daily app has not been changed."
