if exists("b:did_indent")
  finish
endif
let b:did_indent = 1

setlocal indentexpr=CianeIndent()
setlocal indentkeys=0},0],0),!^F,o,O,e

if exists("*CianeIndent")
  finish
endif

function! CianeIndent() abort
  let lnum = prevnonblank(v:lnum - 1)
  if lnum == 0
    return 0
  endif

  let prev = getline(lnum)
  let curr = getline(v:lnum)
  let ind  = indent(lnum)

  " Increase indent after an opener at end of line (ignoring trailing comments)
  if prev =~# '[{(\[]\s*\%(#.*\)\?$'
    let ind += shiftwidth()
  endif

  " Decrease indent when current line begins with a closer
  if curr =~# '^\s*[})\]]'
    let ind -= shiftwidth()
  endif

  return max([ind, 0])
endfunction
