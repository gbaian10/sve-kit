// Runs synchronously in <head> before the stylesheet so a stored theme never flashes the system one.
// Plain script, not a module: it must not wait for the bundle. Mirrors resolveTheme() in
// src/domain/theme.ts and applyThemeAttributes() in src/app/theme-attributes.ts; a test keeps them in sync.
;(function () {
  var THEMES = ["light", "dark"]
  var ACCENTS = ["amber", "teal", "red"]
  var theme = null
  var accent = null
  try {
    var raw = localStorage.getItem("sve-kit:prefs")
    if (raw) {
      var prefs = JSON.parse(raw)
      if (prefs && typeof prefs === "object" && prefs.v === 1) {
        if (THEMES.indexOf(prefs.theme) !== -1) theme = prefs.theme
        if (ACCENTS.indexOf(prefs.accent) !== -1) accent = prefs.accent
      }
    }
  } catch (error) {
    // Storage blocked or corrupt: follow the system theme.
  }
  var root = document.documentElement
  if (theme) root.setAttribute("data-theme", theme)
  else root.removeAttribute("data-theme")
  if (accent) root.setAttribute("data-accent", accent)
  else root.removeAttribute("data-accent")
})()
