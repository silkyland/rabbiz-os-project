.pragma library
// Calc — pure expression evaluator for the in-house Calculator app (no Qt; tested with node).
// Grammar (recursive descent):
//   expr    := term (('+' | '-') term)*
//   term    := percent (('*' | '/') percent)*
//   percent := unary '%'?            (postfix: divides by 100)
//   unary   := ('-' | '+') unary | primary
//   primary := number | '(' expr ')'
// Percent is applied to the preceding value (`200*10%` = 20), matching phone calculators.
// Errors return { ok: false, error } instead of throwing, so the UI can show "Error".

function _tokenize(src) {
    var out = [];
    var i = 0;
    while (i < src.length) {
        var c = src.charAt(i);
        if (c === " ") { i++; continue; }
        if (c >= "0" && c <= "9" || c === ".") {
            var start = i;
            var dots = 0;
            while (i < src.length && (src.charAt(i) >= "0" && src.charAt(i) <= "9" || src.charAt(i) === ".")) {
                if (src.charAt(i) === ".") dots++;
                i++;
            }
            if (dots > 1) return null;
            out.push({ t: "num", v: parseFloat(src.slice(start, i)) });
            continue;
        }
        if ("+-*/%()".indexOf(c) >= 0) { out.push({ t: c }); i++; continue; }
        return null;
    }
    return out;
}

function evaluate(src) {
    var toks = _tokenize(String(src || ""));
    if (toks === null || toks.length === 0)
        return { ok: false, error: "Invalid expression" };
    var pos = 0;

    function peek() { return pos < toks.length ? toks[pos] : null; }
    function eat(t) { if (peek() && peek().t === t) { pos++; return true; } return false; }

    function parseExpr() {
        var v = parseTerm();
        if (v === null) return null;
        while (true) {
            if (eat("+")) {
                var r = parseTerm(); if (r === null) return null; v = v + r;
            } else if (eat("-")) {
                var r2 = parseTerm(); if (r2 === null) return null; v = v - r2;
            } else break;
        }
        return v;
    }
    function parsePercent() {
        var v = parseUnary();
        if (v === null) return null;
        while (eat("%")) v = v / 100;      // postfix: 10% -> 0.1
        return v;
    }
    function parseTerm() {
        var v = parsePercent();
        if (v === null) return null;
        while (true) {
            if (eat("*")) {
                var r = parsePercent(); if (r === null) return null; v = v * r;
            } else if (eat("/")) {
                var r2 = parsePercent(); if (r2 === null) return null;
                if (r2 === 0) return null;
                v = v / r2;
            } else break;
        }
        return v;
    }
    function parseUnary() {
        if (eat("-")) { var v = parseUnary(); return v === null ? null : -v; }
        if (eat("+")) return parseUnary();
        return parsePrimary();
    }
    function parsePrimary() {
        var tok = peek();
        if (!tok) return null;
        if (tok.t === "num") { pos++; return tok.v; }
        if (eat("(")) {
            var v = parseExpr();
            if (v === null || !eat(")")) return null;
            return v;
        }
        return null;
    }

    var value = parseExpr();
    if (value === null || pos !== toks.length)
        return { ok: false, error: "Invalid expression" };
    if (!isFinite(value))
        return { ok: false, error: "Cannot divide by zero" };
    return { ok: true, value: value };
}

// Display formatting: at most 12 significant characters, trailing zeros trimmed.
function format(value) {
    if (!isFinite(value)) return "Error";
    var abs = Math.abs(value);
    if (abs !== 0 && (abs >= 1e12 || abs < 1e-9)) return value.toExponential(5);
    var s = String(Math.round(value * 1e10) / 1e10);
    return s;
}
