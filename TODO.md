# Katla Engine TODO

## Editor

- [ ] Add click-to-focus for editor UI panels -- currently the center panel requires a click before it receives any input. Panels should gain focus automatically on click rather than requiring a separate focus step.
- [ ] Wire up Ctrl+S save shortcut in the editor -- scene save/load works via code but there is no keyboard shortcut to trigger save while editing.
- [ ] Audit and clean up top menu bar -- most menu items are no-ops: File > New Scene, File > Open..., File > Save, File > Quit, Edit > Undo, Edit > Redo, Help > About all just close the dropdown without doing anything. Either implement them or remove them to avoid confusion. DuplicateEntity and ResetParticleSystem are also stubs logged as "not yet implemented".
