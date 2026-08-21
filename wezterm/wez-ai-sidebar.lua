local wezterm = require('wezterm')
local M = {}

local installed = false
local options = {}
local known_tabs = {}
local disabled_tabs = {}
local pending_tabs = {}

local function merge(defaults, supplied)
  local result = {}
  for key, value in pairs(defaults) do result[key] = value end
  for key, value in pairs(supplied or {}) do result[key] = value end
  return result
end

local function is_sidebar(pane)
  local title = string.lower(pane:get_title() or '')
  local process = string.lower(pane:get_foreground_process_name() or '')
  return string.find(title, 'wez%-ai%-sidebar') ~= nil
    or string.find(process, 'wez%-ai%-sidebar') ~= nil
end

local function find_sidebar(tab)
  for _, pane in ipairs(tab:panes()) do
    if is_sidebar(pane) then return pane end
  end
  return nil
end

local function ensure_tab(tab)
  local id = tab:tab_id()
  if disabled_tabs[id] or pending_tabs[id] then return end
  if find_sidebar(tab) then
    known_tabs[id] = true
    return
  end
  -- Once a sidebar existed, a clean TUI exit (`q`) is treated as an intentional
  -- close. Ctrl+Shift+A clears this flag and recreates it.
  if known_tabs[id] then return end
  local target = tab:active_pane()
  if not target or is_sidebar(target) then return end
  pending_tabs[id] = true
  local ok, sidebar = pcall(function()
    return target:split({
      direction = options.position == 'right' and 'Right' or 'Left',
      top_level = true,
      size = options.width,
      args = { options.binary },
      set_environment_variables = {
        WEZ_AI_SIDEBAR = '1',
        WEZ_AI_SIDEBAR_TAB = tostring(id),
      },
    })
  end)
  pending_tabs[id] = nil
  if ok and sidebar then
    known_tabs[id] = true
    target:activate()
  else
    wezterm.log_error('wez-ai-sidebar: failed to create sidebar: ' .. tostring(sidebar))
  end
end

local function ensure_window(window)
  if not options.auto_create then return end
  for _, tab in ipairs(window:mux_window():tabs()) do ensure_tab(tab) end
end

local function toggle(window, pane)
  local tab = pane:tab()
  local id = tab:tab_id()
  local sidebar = find_sidebar(tab)
  if sidebar then
    disabled_tabs[id] = true
    known_tabs[id] = true
    window:perform_action(wezterm.action.CloseCurrentPane({ confirm = false }), sidebar)
  else
    disabled_tabs[id] = nil
    known_tabs[id] = nil
    ensure_tab(tab)
  end
end

local function spawn_tab(window, pane, args)
  local command = args and { args = args } or {}
  local cwd = pane and pane:get_current_working_dir() or nil
  if cwd then command.cwd = cwd end
  local tab, child = window:mux_window():spawn_tab(command)
  known_tabs[tab:tab_id()] = nil
  disabled_tabs[tab:tab_id()] = nil
  ensure_tab(tab)
  child:activate()
end

function M.setup(config, supplied)
  options = merge({
    width = 26,
    position = 'left',
    auto_create = true,
    binary = 'wez-ai-sidebar',
    toggle_key = 'A',
    toggle_mods = 'CTRL|SHIFT',
    launch_shortcuts = false,
  }, supplied)

  config.keys = config.keys or {}
  table.insert(config.keys, {
    key = options.toggle_key,
    mods = options.toggle_mods,
    action = wezterm.action_callback(toggle),
  })
  table.insert(config.keys, {
    key = 'T',
    mods = 'CTRL|SHIFT',
    action = wezterm.action_callback(function(window, pane) spawn_tab(window, pane, nil) end),
  })

  if options.launch_shortcuts then
    local launches = options.launch_shortcuts == true and {
      C = { 'claude' }, O = { 'opencode' }, X = { 'codex' },
    } or options.launch_shortcuts
    for key, args in pairs(launches) do
      table.insert(config.keys, {
        key = key,
        mods = 'CTRL|SHIFT|ALT',
        action = wezterm.action_callback(function(window, pane) spawn_tab(window, pane, args) end),
      })
    end
  end

  if not installed then
    installed = true
    wezterm.on('update-status', function(window, _pane) ensure_window(window) end)
    wezterm.on('window-config-reloaded', function(window, _pane) ensure_window(window) end)
    wezterm.on('new-tab-button-click', function(window, pane, button, _default_action)
      if button == 'Left' then
        spawn_tab(window, pane, nil)
        return false
      end
    end)
  end
  return config
end

return M
