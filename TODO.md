# TODO

## Particle System Usability

- [ ] Fix position duplication between Transform and EmitterConfig — ParticleSystem::update should read the entity's TransformComponent and override config.position so emitters follow their entity when moved
- [ ] Wrap EmitterConfig GPU padding behind a user-facing builder or separate user config struct — users should not need to set _pad_position, _pad_velocity, _pad_color, _pad_forces manually
- [ ] Change EmitterConfig.shape from raw u32 to EmitterShape enum — provide direct field access without requiring set_shape()/get_shape() helpers
- [ ] Fix with_line_shape axis parameter being silently ignored — either implement axis support in the shader or remove the parameter
- [ ] Add editing controls to the ParticleInspector — sliders/drag fields for emit rate, lifetime, scale, color, forces etc. instead of read-only text rows
- [ ] Add built-in preset factory functions — EmitterPreset::fire(), EmitterPreset::smoke(), EmitterPreset::sparks() etc. with sensible defaults
- [ ] Change ParticleSystem::update to take &mut GlobalParticleSystem instead of &mut Option<GlobalParticleSystem> — avoids silent skip and awkward call sites
- [ ] Add per-emitter alive count feedback — allow querying actual alive particle count per emitter, not just theoretical estimated_max_alive
- [ ] Add kill-all-particles-on-destroy option — optionally immediately kill all living particles when an emitter is destroyed instead of letting them expire naturally
- [ ] Add color over lifetime and size over lifetime curves — enable fire (bright to dark), smoke (opaque to transparent), sparks (big to small) effects without shader modifications
