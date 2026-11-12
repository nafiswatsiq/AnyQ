# PoC Result: Overlay Capture Protection

Status: Pending manual verification on a real Windows 10/11 desktop with Zoom/Meet/OBS installed.

## What was implemented
- The app now creates a transparent, borderless, always-on-top `live` window.
- The window uses content protection and cursor-event ignoring as the first stealth layer.
- The global hotkey `Ctrl+Shift+Space` toggles click-through state and emits a frontend signal for UI feedback.
- Opacity is managed in the frontend CSS layer as requested.

## Why this is not marked as fully passed
This environment does not provide a real Windows desktop session with screen sharing tools running, so the capture-suppression behavior cannot be empirically validated here. Per the project specification, this is a manual verification item, not an automated pass.

## Required manual test procedure
1. Run the app on Windows 10/11.
2. Open Zoom or Google Meet and start a screen share for "Entire Screen".
3. Ensure the overlay is shown and the hotkey toggles click-through.
4. Verify the overlay remains visible on the local monitor but does not appear in the shared feed or broadcast output.
5. Repeat in OBS with Display Capture and Window Capture modes.
6. Confirm the overlay disappears from captured output while remaining visible locally.

## Expected result
- Local user sees the overlay normally.
- Screen-capture streams show no overlay content.
- The app remains stable and does not crash when the stealth toggle is changed.

## Notes
This result should be revalidated after the app is built and launched on an actual Windows machine using the required conferencing software. The implementation is ready for that manual verification step.
