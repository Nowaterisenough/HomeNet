# Reference UI Restore Design

## Goal

Recreate `docs/34a8eba5-24bc-46b3-a96d-e44e1574c810.png` as the primary desktop UI for the HomeNet Tauri/Vue app while preserving the existing DDNS, forwarding-rule, and log interactions.

## Approved Scope

- Match the screenshot at the 1440x900 desktop Tauri window size.
- Keep real Tauri backend calls in the existing Vue components.
- Use screenshot data as first-screen fallback data when backend responses are empty or unavailable.
- Preserve the current single-page overview composition: status cards, DDNS form, forwarding rules table/editor, and recent logs.
- Mobile responsiveness only needs to avoid broken layout; pixel parity is desktop-focused.

## Visual Requirements

- Add a Windows-style app title bar with a menu icon, title text, and window-control glyphs.
- Convert the left navigation to the light sidebar shown in the reference.
- Add a sidebar service-status block with version, running indicator, and auto-start toggle.
- Use compact white cards on a pale gray background with subtle borders and shadows.
- Recreate five top status cards with blue/green icon discs, large values, and short subtitles.
- Recreate the two middle panels:
  - Left: Aliyun DDNS form with enabled/running chips, fields, action buttons, and footer status.
  - Right: IPv6/IPv4 forwarding rules table, toolbar, selected-row highlight, and inline editor.
- Recreate the bottom recent logs table with compact rows and a clear button.

## Functional Requirements

- `DdnsPanel.vue` keeps loading/saving/testing/updating via existing Tauri commands.
- `ForwardRulesPanel.vue` keeps listing, saving, deleting, and enabling rules via existing Tauri commands.
- `LogPanel.vue` keeps auto-refreshing and clearing logs via existing Tauri commands.
- `App.vue` may load runtime status if the backend command is available; fallback values must match the reference screenshot.

## Implementation Shape

- Keep current component boundaries.
- Replace corrupted Chinese text in templates with correct Chinese labels.
- Replace emoji structural icons with CSS or text glyphs/SVG-like CSS marks so the UI is stable across platforms.
- Keep styling scoped to the component or app root variables.
- Do not introduce a new UI library.

## Verification

- Run `pnpm build`.
- Start/keep the dev app and inspect `http://localhost:1420`.
- Capture a desktop screenshot if tooling is available and compare against the reference for major layout, density, and visual hierarchy.
