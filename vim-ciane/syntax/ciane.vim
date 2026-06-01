if exists("b:current_syntax")
  finish
endif

let s:save_cpo = &cpo
set cpo&vim

" ── Comments ──────────────────────────────────────────────────────────────────
syntax match  cianeComment  "#.*$"              contains=cianeTodo
syntax keyword cianeTodo    TODO FIXME NOTE HACK contained

" ── Top-level keywords ────────────────────────────────────────────────────────
" Each keyword starts a nextgroup chain so the correct block type is used
" downstream (structural vs shell).
syntax keyword cianeUseKw      use      nextgroup=cianeUseName       skipwhite
syntax keyword cianeStageKw    stage    nextgroup=cianeStageName     skipwhite
syntax keyword cianeDefaultsKw defaults nextgroup=cianeDefaultsAttrs skipwhite

" ── Stage ─────────────────────────────────────────────────────────────────────
syntax match cianeStageName "\<[a-zA-Z_][a-zA-Z0-9_-]*\>"
      \ contained
      \ nextgroup=cianeStageAttrs,cianeStageBlock skipwhite skipnl

syntax region cianeStageAttrs matchgroup=cianeParen start="(" end=")"
      \ contained nextgroup=cianeStageBlock skipwhite skipnl
      \ contains=cianeAttrKey,cianeEq,cianeBareValue,cianeNumber,cianeBracketList,cianeComma,cianeComment
      \ fold

" Stage body: may only contain job/template definitions.
syntax region cianeStageBlock matchgroup=cianeBrace start="{" end="}"
      \ contained
      \ contains=cianeJobKw,cianeTemplateKw,cianeComment
      \ fold

" ── Job ───────────────────────────────────────────────────────────────────────
syntax keyword cianeJobKw job nextgroup=cianeJobName skipwhite contained

syntax match cianeJobName "\<[a-zA-Z_][a-zA-Z0-9_-]*\>"
      \ contained
      \ nextgroup=cianeJobAttrs,cianeShellBlock,cianeBracketList skipwhite skipnl

syntax region cianeJobAttrs matchgroup=cianeParen start="(" end=")"
      \ contained nextgroup=cianeShellBlock,cianeBracketList skipwhite skipnl
      \ contains=cianeAttrKey,cianeEq,cianeBareValue,cianeNumber,cianeBracketList,cianeComma,cianeComment
      \ fold

" ── Template ──────────────────────────────────────────────────────────────────
syntax keyword cianeTemplateKw template nextgroup=cianeTemplateName skipwhite contained

syntax match cianeTemplateName "\<[a-zA-Z_][a-zA-Z0-9_-]*\>"
      \ contained
      \ nextgroup=cianeTemplateAttrs,cianeBracketList skipwhite skipnl

syntax region cianeTemplateAttrs matchgroup=cianeParen start="(" end=")"
      \ contained nextgroup=cianeBracketList skipwhite skipnl
      \ contains=cianeAttrKey,cianeEq,cianeBareValue,cianeNumber,cianeBracketList,cianeComma,cianeComment
      \ fold

" ── Step list / ref list: [ ... ] ─────────────────────────────────────────────
" Used for job step-lists, template bodies, and dependency ref-lists in attrs.
syntax region cianeBracketList matchgroup=cianeBracket start="\[" end="\]"
      \ contained
      \ contains=cianeStepKw,cianeStepsKw,cianeRef,cianeRefSep,cianeComma,cianeComment,cianeShellBlock
      \ fold

" ── Step ──────────────────────────────────────────────────────────────────────
syntax keyword cianeStepKw  step  nextgroup=cianeStepName skipwhite contained
syntax keyword cianeStepsKw steps                                    contained

syntax match cianeStepName "\<[a-zA-Z_][a-zA-Z0-9_-]*\>"
      \ contained
      \ nextgroup=cianeShellBlock skipwhite skipnl

" ── Shell block: job inline body and step body — nothing highlighted inside ───
syntax region cianeShellBlock matchgroup=cianeBrace start="{" end="}"
      \ contained contains=NONE fold

" ── Workflow def (top-level) ─────────────────────────────────────────────────
syntax keyword cianeWorkflowKw workflow
      \ nextgroup=cianeWorkflowDefName
      \ skipwhite

syntax match cianeWorkflowDefName "\<[a-zA-Z_][a-zA-Z0-9_-]*\>"
      \ contained
      \ nextgroup=cianeWorkflowDefAttrs,cianeWorkflowDefBlock
      \ skipwhite skipnl

syntax region cianeWorkflowDefAttrs matchgroup=cianeParen start="(" end=")"
      \ contained nextgroup=cianeWorkflowDefBlock skipwhite skipnl
      \ contains=cianeAttrKey,cianeEq,cianeBareValue,cianeNumber,cianeComma,cianeComment
      \ fold

syntax region cianeWorkflowDefBlock matchgroup=cianeBrace start="{" end="}"
      \ contained
      \ contains=cianeUseKw,cianeStageKw,cianeTemplateKw,cianeDefaultsKw,cianeComment
      \ fold

" ── Use decl ─────────────────────────────────────────────────────────────────
syntax match cianeUseName "\<[a-zA-Z_][a-zA-Z0-9_-]*\>"
      \ contained
      \ nextgroup=cianeUseAttrs skipwhite skipnl

syntax region cianeUseAttrs matchgroup=cianeParen start="(" end=")"
      \ contained
      \ contains=cianeAttrKey,cianeEq,cianeBareValue,cianeNumber,cianeComma,cianeComment
      \ fold

" ── Defaults ──────────────────────────────────────────────────────────────────
syntax region cianeDefaultsAttrs matchgroup=cianeParen start="(" end=")"
      \ contained
      \ contains=cianeAttrKey,cianeEq,cianeBareValue,cianeNumber,cianeComma,cianeComment
      \ fold

" ── Attribute elements ────────────────────────────────────────────────────────
syntax match cianeEq        "="                           contained
syntax match cianeComma     ","                           contained
" cianeBareValue before cianeNumber: for equal-length matches (e.g. "300"),
" cianeNumber is defined later and takes priority per vim's last-defined rule.
syntax match cianeBareValue "[^=,)(\[\n\r\t ]\+"          contained
syntax match cianeNumber    "\<[0-9]\+\%(\.[0-9]\+\)\?\>" contained
" cianeAttrKey after cianeBareValue so equal-length matches prefer the key.
" \ze ends the match before the = so cianeEq still highlights it.
syntax match cianeAttrKey   "\<[a-zA-Z_][a-zA-Z0-9_]*\>\ze\s*=" contained

syntax match cianeRef    "\<[a-zA-Z_][a-zA-Z0-9_-]*\>" contained
syntax match cianeRefSep "[./]"                          contained

" ── Highlight links ───────────────────────────────────────────────────────────
highlight default link cianeUseKw      Keyword
highlight default link cianeStageKw    Keyword
highlight default link cianeDefaultsKw Keyword
highlight default link cianeJobKw      Keyword
highlight default link cianeTemplateKw Keyword
highlight default link cianeStepKw     Keyword
highlight default link cianeStepsKw    Keyword
highlight default link cianeWorkflowKw Keyword

highlight default link cianeWorkflowDefName Function
highlight default link cianeUseName      Function
highlight default link cianeStageName    Function
highlight default link cianeJobName      Function
highlight default link cianeTemplateName Function
highlight default link cianeStepName     Function

highlight default link cianeAttrKey    Identifier
highlight default link cianeEq         Operator
highlight default link cianeBareValue  String
highlight default link cianeNumber     Number
highlight default link cianeRef        Identifier
highlight default link cianeRefSep     Delimiter
highlight default link cianeComma      Delimiter
highlight default link cianeParen      Delimiter
highlight default link cianeBracket    Delimiter
highlight default link cianeBrace      Delimiter
highlight default link cianeComment    Comment
highlight default link cianeTodo       Todo

let b:current_syntax = "ciane"

let &cpo = s:save_cpo
unlet s:save_cpo
