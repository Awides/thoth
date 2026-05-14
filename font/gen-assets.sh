#!/usr/bin/env bash
set -euo pipefail

# Generate Android icon and splash drawables for Thoth.
#
# Prerequisites:
#   - ImageMagick (convert)
#   - Iosevka custom fonts built at $IOSEVKA_DIR/dist/
#
# Usage:
#   ./gen-assets.sh [ANDROID_RES_DIR] [IOSEVKA_DIR]
#
# Defaults:
#   ANDROID_RES_DIR = target/dx/thoth/debug/android/app/app/src/main/res
#   IOSEVKA_DIR = ../../iosevka (sibling to thoth)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
THOTH_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

ANDROID_RES_DIR="${1:-$THOTH_DIR/target/dx/thoth/debug/android/app/app/src/main/res}"
IOSEVKA_DIR="${2:-$THOTH_DIR/../iosevka}"

FONT_HEAVY="$IOSEVKA_DIR/dist/MsgSans/TTF/MsgSans-Heavy.ttf"
FONT_THIN="$IOSEVKA_DIR/dist/MsgSans/TTF/MsgSans-Thin.ttf"
FONT_HEAVY_OBL="$IOSEVKA_DIR/dist/MsgSans/TTF/MsgSans-HeavyOblique.ttf"
FONT_THIN_OBL="$IOSEVKA_DIR/dist/MsgSans/TTF/MsgSans-ThinOblique.ttf"

BG="#0d0d0d"
FG="#ededed"

for f in "$FONT_HEAVY" "$FONT_THIN" "$FONT_HEAVY_OBL" "$FONT_THIN_OBL"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: Font not found: $f"
        echo "Build Iosevka first: cd $IOSEVKA_DIR && npm run build -- --jCmd=2 contents::MsgSans"
        exit 1
    fi
done

if ! command -v convert &>/dev/null; then
    echo "ERROR: ImageMagick (convert) not found"
    exit 1
fi

mkdir -p "$ANDROID_RES_DIR"/{drawable,drawable-mdpi,drawable-hdpi,drawable-xhdpi,drawable-xxhdpi,drawable-xxxhdpi,mipmap-mdpi,mipmap-hdpi,mipmap-xhdpi,mipmap-xxhdpi,mipmap-xxxhdpi}

echo "Generating adaptive-icon layers..."

# --- Adaptive Icon Background (108dp, solid dark) ---
# The background is just a solid color — use a vector drawable for crispness
cat > "$ANDROID_RES_DIR/drawable/ic_launcher_background.xml" << 'XML'
<?xml version="1.0" encoding="utf-8"?>
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp"
    android:height="108dp"
    android:viewportWidth="108"
    android:viewportHeight="108">
    <path
        android:fillColor="#0d0d0d"
        android:pathData="M0,0h108v108h-108z" />
</vector>
XML

# --- Adaptive Icon Foreground (▷ centered, 108dp safe zone 72dp) ---
# Render the ▷ glyph as a high-res PNG, then create a vector-like foreground
# The adaptive icon foreground must be 108x108dp with content in the center 72x72dp
# We render at 432x432 (xxxhdpi = 4x) and scale down for each density

ICON_SIZE=432
ICON_PT=180

echo "  Rendering icon foreground (▷)..."
convert -size "${ICON_SIZE}x${ICON_SIZE}" xc:none \
    -font "$FONT_HEAVY" \
    -pointsize "$ICON_PT" \
    -fill "$FG" \
    -gravity center \
    -annotate 0 '▷' \
    "$ANDROID_RES_DIR/drawable/ic_launcher_foreground.webp"

# --- Mipmap icons (full icon with background) for pre-API 26 ---
# These include the background circle/square plus the foreground glyph
# Android requires specific pixel sizes per density:
#   mdpi=48, hdpi=72, xhdpi=96, xxhdpi=144, xxxhdpi=192

declare -A DENSITIES=(
    [mdpi]=48
    [hdpi]=72
    [xhdpi]=96
    [xxhdpi]=144
    [xxxhdpi]=192
)

for density in "${!DENSITIES[@]}"; do
    size="${DENSITIES[$density]}"
    pt=$((size * 50 / 100))
    dir="$ANDROID_RES_DIR/mipmap-$density"

    echo "  Rendering mipmap-$density (${size}x${size})..."
    convert -size "${size}x${size}" xc:"$BG" \
        -font "$FONT_HEAVY" \
        -pointsize "$pt" \
        -fill "$FG" \
        -gravity center \
        -annotate 0 '▷' \
        "$dir/ic_launcher.webp"
done

# --- Splash screen drawable ---
# Rendered at multiple densities. The splash shows "THOTH▷" in oblique.
# Android 12+ uses the splash screen API; older versions use window background.
# We generate a simple centered text splash as a 9-patch alternative.

echo "Generating splash drawables..."

SPLASH_SIZE=1080
SPLASH_PT_HEAVY=72
SPLASH_PT_THIN=36

# Full splash for xxxhdpi
echo "  Rendering splash (THOTH▷)..."
convert -size "${SPLASH_SIZE}x${SPLASH_SIZE}" xc:"$BG" \
    -font "$FONT_HEAVY_OBL" \
    -pointsize "$SPLASH_PT_HEAVY" \
    -fill "$FG" \
    -gravity center \
    -annotate 0 'THOTH▷' \
    "$ANDROID_RES_DIR/drawable-xxxhdpi/splash.png"

# Scale down for other densities
for density in xxhdpi xhdpi hdpi mdpi; do
    case $density in
        xxhdpi) scale=0.75;;
        xhdpi)  scale=0.5;;
        hdpi)   scale=0.33;;
        mdpi)   scale=0.25;;
    esac
    w=$(python3 -c "print(int($SPLASH_SIZE * $scale))")
    dir="$ANDROID_RES_DIR/drawable-$density"
    mkdir -p "$dir"
    echo "  Rendering splash $density (${w}x${w})..."
    convert -size "${w}x${w}" xc:"$BG" \
        -font "$FONT_HEAVY_OBL" \
        -pointsize $(python3 -c "print(int($SPLASH_PT_HEAVY * $scale))") \
        -fill "$FG" \
        -gravity center \
        -annotate 0 'THOTH▷' \
        "$dir/splash.png"
done

echo "Done. Assets written to $ANDROID_RES_DIR"
