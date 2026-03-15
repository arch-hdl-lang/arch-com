" Vim filetype plugin for the ARCH HDL language

if exists("b:did_ftplugin")
  finish
endif
let b:did_ftplugin = 1

" Indentation: 2 spaces, no tabs
setlocal expandtab
setlocal shiftwidth=2
setlocal softtabstop=2
setlocal tabstop=2

" Line comments
setlocal commentstring=//\ %s
setlocal comments=://

" Don't wrap long lines (hardware descriptions are often wide)
setlocal textwidth=0

" Matching pairs: angle brackets for types like UInt<32>
setlocal matchpairs+=<:>

" Fold on construct blocks (end keyword Name)
setlocal foldmethod=syntax

let b:undo_ftplugin = "setl et< sw< sts< ts< cms< com< tw< mps< fdm<"
