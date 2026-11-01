// Browser-local UI preferences (localStorage `wingman_setting_<key>`). This
// module imports nothing, so a module may read a setting while it is still
// being evaluated, whatever the evaluation order of the module graph.
export const SETTING_DEFAULTS = {
  suppress_hid_timeout: false,
  capture_pulse: true,
  release_key_toast: true,
  render_pixelated: true,
  auto_expand_inspector: true,
  target_layout: 'us',
};
export function getSetting(key, fallback = null) {
  try {
    const v = localStorage.getItem(`wingman_setting_${key}`);
    if (v === null) return fallback !== null ? fallback : SETTING_DEFAULTS[key];
    if (v === 'true') return true;
    if (v === 'false') return false;
    return v;
  } catch (_) {
    return fallback !== null ? fallback : SETTING_DEFAULTS[key];
  }
}
export function setSetting(key, value) {
  try {
    localStorage.setItem(`wingman_setting_${key}`, String(value));
  } catch (_) {}
}
