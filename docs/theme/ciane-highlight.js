/*
 * Ciane syntax highlighting for highlight.js (as bundled by mdBook).
 *
 * Mirrors the Vim (vim-ciane/syntax/ciane.vim) and VSCode
 * (vscode-ciane/syntaxes/ciane.tmLanguage.json) grammars:
 *
 *   - keywords: workflow, use, stage, job, template, step, steps, defaults, unset
 *   - definition names (workflow/use/stage/job/template/step) are titles
 *   - `#` line comments
 *   - attribute keys, bare values (strings), numbers, ref lists (stage.job)
 *   - shell bodies `{ ... }` are not highlighted, `${...}` nests inside them
 *   - return annotations `-> [ paths, $VARS ]`
 */

/* global hljs */

(function () {
    "use strict";

    if (typeof hljs === "undefined") {
        return;
    }

    hljs.registerLanguage("ciane", function (hljs) {
        const IDENT = "[a-zA-Z_][a-zA-Z0-9_-]*";

        const COMMENT = hljs.HASH_COMMENT_MODE;

        const TITLE = {
            className: "title",
            begin: new RegExp(IDENT),
        };

        const NUMBER = {
            className: "number",
            begin: /\b[0-9]+(\.[0-9]+)?\b/,
        };

        // $VAR in a return annotation list.
        const ENV_VAR = {
            className: "variable",
            begin: /\$[a-zA-Z_][a-zA-Z0-9_]*/,
        };

        // Reference such as `rust_slim`, `build.build_release` or `import/stage.template`.
        const REF = {
            className: "variable",
            begin: new RegExp(IDENT + "([./]" + IDENT + ")*"),
        };

        // ${...} inside a shell body: consumed whole so its `}` doesn't end the body.
        const SHELL_SUBST = {
            begin: /\$\{/,
            end: /\}/,
        };

        // Shell body: job/step inline body — nothing highlighted inside. The
        // closing `}` is left for the containing job/step to consume.
        const SHELL_BODY = {
            begin: /\{/,
            end: /(?=\})/,
            contains: [SHELL_SUBST],
        };

        const STEP = {
            begin: "\\bstep\\s+" + IDENT,
            returnBegin: true,
            end: /\}|(?=[,\]])/,
            contains: [
                { className: "keyword", begin: /\bstep\b/ },
                COMMENT,
                SHELL_BODY,
                TITLE,
            ],
        };

        // Step list / step ref list: [ step a { ... }, steps, step_ref ]
        const STEP_LIST = {
            begin: /\[/,
            end: /(?=\])/,
            contains: [
                COMMENT,
                { className: "keyword", begin: /\bsteps\b/ },
                STEP,
                REF,
            ],
        };

        // Reference list in attributes: inherit = [ rust_slim, base ]
        const REF_LIST = {
            begin: /\[/,
            end: /\]/,
            contains: [COMMENT, REF],
        };

        // Attribute list: ( key = value, ... ). Nests for variables = ( KEY = value, ... ).
        const ATTR_LIST = {
            begin: /\(/,
            end: /\)/,
            contains: [
                COMMENT,
                { className: "keyword", begin: /\bunset\b/ },
                { className: "attr", begin: /[a-zA-Z_][a-zA-Z0-9_]*(?=\s*=)/ },
                { className: "operator", begin: /=/ },
                REF_LIST,
                "self",
                NUMBER,
                { className: "string", begin: /[^\s,()\[\]=]+/ },
            ],
        };

        // Return annotation: -> [ paths, $VARS ]
        const RETURN_ANNOTATION = {
            begin: /->\s*\[/,
            end: /\]/,
            contains: [
                COMMENT,
                ENV_VAR,
                { className: "string", begin: /[^,\]\s]+/ },
            ],
        };

        const ARROW = {
            className: "operator",
            begin: /->/,
        };

        const JOB = {
            begin: "\\bjob\\s+" + IDENT,
            returnBegin: true,
            end: /\}|\]/,
            contains: [
                { className: "keyword", begin: /\bjob\b/ },
                COMMENT,
                ATTR_LIST,
                STEP_LIST,
                SHELL_BODY,
                TITLE,
            ],
        };

        const TEMPLATE = {
            begin: "\\btemplate\\s+" + IDENT,
            returnBegin: true,
            end: /\}|\]|(?=\n)/,
            contains: [
                { className: "keyword", begin: /\btemplate\b/ },
                COMMENT,
                ATTR_LIST,
                STEP_LIST,
                SHELL_BODY,
                TITLE,
            ],
        };

        const STAGE = {
            begin: "\\bstage\\s+" + IDENT,
            returnBegin: true,
            end: /\}/,
            contains: [
                { className: "keyword", begin: /\bstage\b/ },
                COMMENT,
                ATTR_LIST,
                RETURN_ANNOTATION,
                ARROW,
                JOB,
                TEMPLATE,
                TITLE,
            ],
        };

        // workflow and use are single-line declarations; their attribute lists
        // are handled by the top-level ATTR_LIST.
        const DECL = {
            begin: "\\b(?:workflow|use)\\s+" + IDENT,
            returnBegin: true,
            end: /(?=[(\n{])/,
            contains: [
                { className: "keyword", begin: /\b(?:workflow|use)\b/ },
                TITLE,
            ],
        };

        return {
            name: "ciane",
            aliases: ["ci"],
            contains: [
                COMMENT,
                DECL,
                { className: "keyword", begin: /\bdefaults\b/ },
                STAGE,
                JOB,
                TEMPLATE,
                ATTR_LIST,
                RETURN_ANNOTATION,
                ARROW,
            ],
        };
    });

    // mdBook's book.js performs its highlighting pass before this file is
    // loaded, so `ciane` blocks were already skipped (unknown language at
    // that point). Highlight them now that the language is registered.
    // Blocks that somehow got highlighted already contain spans; skip those.
    document
        .querySelectorAll("pre code.language-ciane, pre code.language-ci")
        .forEach(function (block) {
            if (block.children.length === 0) {
                hljs.highlightBlock(block);
            }
        });
})();
