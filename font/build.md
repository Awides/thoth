# Font Build Instructions

## Custom Iosevka Builds

Source: https://github.com/be5invis/Iosevka

```bash
# Clone (shallow) to a sibling directory
git clone --depth 1 https://github.com/be5invis/Iosevka.git ~/dev/iosevka
cd ~/dev/iosevka

# Copy our build plans
cp ~/dev/thoth/font/private-build-plans.toml .

# Install deps
npm install

# Build each family (one at a time to avoid OOM)
# --jCmd=2 limits parallel threads
npm run build -- --jCmd=2 contents::MsgSans
npm run build -- --jCmd=2 contents::MsgSlab
npm run build -- --jCmd=2 contents::SystemSans
npm run build -- --jCmd=2 contents::SystemSlab

# Copy WOFF2 output to Thoth assets
cp dist/*/WOFF2/*.woff2 ~/dev/thoth/assets/fonts/
```

## Font Families

| Family | Spacing | Serifs | Use |
|--------|---------|--------|-----|
| MsgSans | quasi-proportional | sans | Chat messages, UI text, body copy |
| MsgSlab | quasi-proportional | slab | Quoted text, long-form, editorial |
| SystemSans | monospace | sans | Code blocks, inline code, terminal |
| SystemSlab | monospace | slab | Code with slab serif variant |

## Weights

| Weight | CSS | Use |
|--------|-----|-----|
| Thin | 100 | Decorative headings, splash |
| ExtraLight | 200 | Metadata, timestamps, muted text |
| Regular | 400 | Body text, messages, code |
| Heavy | 900 | Sender names, emphasis, UI chrome |

## CJK Strategy (Future)

Sarasa Gothic (https://github.com/be5invis/Sarasa-Gothic) combines Iosevka/Inter + Source Han Sans
for CJK, but only supports weights 200-700. Our Latin fonts go 100-900.

Plan: Use CSS `@font-face` with `unicode-range` to serve Iosevka for Latin at 100-900,
and Sarasa or Noto Sans SC for CJK at 200-700. Weights snap to nearest available.
This can be done without rebuilding Iosevka — just add CJK `@font-face` rules.

Pre-built Sarasa releases: https://github.com/be5invis/Sarasa-Gothic/releases
