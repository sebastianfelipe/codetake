# CodeTake brand assets

| File | Use |
| --- | --- |
| `app-icon.png` | Application icon source (1024×1024, macOS icon grid, transparent corners). Every size in `apps/desktop/src-tauri/icons/` is generated from it. |
| `logo-lockup.png` | Icon and "CodeTake" wordmark on a dark background (README banner). |
| `icon-variants.png` | Dark, light, monochrome and glyph-only versions of the icon. |
| `source/` | The original artwork these files were cropped from, unmodified. |

The derived files are crops of the originals with transparency applied
outside the icon's rounded square; the artwork itself is not redrawn.
`scripts/brand-image.swift` performs the crops. To regenerate the
application icons:

```sh
swift scripts/brand-image.swift crop assets/brand/source/app-icon-original.png \
  assets/brand/app-icon.png 161 148 926 926 246 1024 100
pnpm --filter @codetake/desktop tauri icon ../../assets/brand/app-icon.png -o src-tauri/icons
```

The logo is part of the CodeTake project and may be used to refer to
CodeTake. It is distributed with the repository under the same MIT
license as the code.
