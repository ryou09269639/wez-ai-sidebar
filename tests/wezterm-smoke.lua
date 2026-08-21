local wezterm = require('wezterm')
local config = wezterm.config_builder()
local source = debug.getinfo(1, 'S').source:sub(2)
local root = source:match('^(.*)/tests/')
local module = dofile(root .. '/wezterm/wez-ai-sidebar.lua')
module.setup(config, { auto_create = false, width = 26 })

local ctrl_s = 0
for _, binding in ipairs(config.keys or {}) do
  if string.lower(binding.key or '') == 's' and binding.mods == 'CTRL' then
    ctrl_s = ctrl_s + 1
  end
end
assert(ctrl_s == 1, 'Ctrl+S should be added when it is unassigned')

local collision = wezterm.config_builder()
collision.keys = {
  { key = 's', mods = 'CTRL', action = wezterm.action.SendString('existing') },
}
module.setup(collision, { auto_create = false })
ctrl_s = 0
for _, binding in ipairs(collision.keys or {}) do
  if string.lower(binding.key or '') == 's' and binding.mods == 'CTRL' then
    ctrl_s = ctrl_s + 1
  end
end
assert(ctrl_s == 1, 'Ctrl+S should not replace or duplicate an existing assignment')
return config
