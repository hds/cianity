if exists("b:did_ftplugin")
  finish
endif
let b:did_ftplugin = 1
let b:undo_ftplugin = "setl cms< com< sw< sts< et<"

" ── Editing defaults ──────────────────────────────────────────────────────────
setlocal commentstring=#\ %s
setlocal comments=:#
setlocal shiftwidth=4
setlocal softtabstop=4
setlocal expandtab

" ── LSP (Neovim built-in, requires Neovim 0.9+) ──────────────────────────────
if !has('nvim')
  finish
endif

lua << EOF
-- Locate the workspace root by walking up from the current buffer's directory,
-- looking for workflow.ci (preferred) or .workflow.ci.
local function find_root()
  local buf_path = vim.api.nvim_buf_get_name(0)
  local start_dir = vim.fn.fnamemodify(buf_path, ':p:h')

  local markers = vim.fs.find({ 'workflow.ci', '.workflow.ci' }, {
    upward = true,
    path   = start_dir,
  })
  if markers[1] then
    return vim.fn.fnamemodify(markers[1], ':h')
  end
  return vim.fn.getcwd()
end

vim.lsp.start({
  name     = 'cianity',
  cmd      = { 'cianity', 'lsp' },
  root_dir = find_root(),
})
EOF
