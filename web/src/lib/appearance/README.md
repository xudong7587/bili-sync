# Sunny UI in bili-sync

Source: https://github.com/xudong7587/sunny-ui-design-system/tree/de782ec

- `sunny.css`: upstream material styles, with generic tokens prefixed by `sunny-`.
- `presets.ts`: upstream palettes and materials; adds the existing bili pink and teal choices.
- `host.css`: Svelte/shadcn semantic variables and specific component surfaces, controls and sidebar mapping.
- `../theme.ts`: browser preference restoration and synchronization; retains `bili-theme`, adds `bili-material`.
- `../components/appearance-settings.svelte`: independent Svelte options and preview page.

Light/dark mode continues to use mode-watcher, so the sidebar switch and appearance page share one preference. The CSS `data-theme` attribute mirrors its current mode. Appearance settings never update server configuration.

When updating the source, compare upstream CSS and presets, preserve the token namespace mapping, then check actual cards, buttons, fields, dialogs and sidebar in both modes. Do not import the React component into Svelte. Keep the full GPL-3.0-only license and source attribution with copied files.
