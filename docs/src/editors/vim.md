# Vim/NeoVim

There is a Plugin for the Ciane DSL available for Vim and NeoVim.

Currently this plugin isn't set up to be installed from a Git reposity (like GitHub), instead it needs to be comfigures from a local clone of the `cianity` repository.

## vim-plug

```vim
Plug '~/path/to/cianity/vim-ciane'
```

## lazy.nvim

```lua
require("lazy").setup({
    { dir = '~/path/to/cianity/vim-ciane', },
})
```
