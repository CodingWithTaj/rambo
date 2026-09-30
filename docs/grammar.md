# Grammar

The complete syntax of Rambo in [EBNF](https://en.wikipedia.org/wiki/Extended_Backus%E2%80%93Naur_form). `{ x }` means zero or more `x`, `[ x ]` means optional, and quoted text is literal.

## Statements

```text
program      = { statement } ;

statement    = "let" IDENT "=" expression ";"
             | "fn" IDENT params block
             | "if" expression block [ "else" ( ifStatement | block ) ]
             | "while" expression block
             | "for" IDENT "in" expression [ ".." expression ] block
             | "return" [ expression ] ";"
             | "break" ";"
             | "continue" ";"
             | block
             | expression [ assignOp expression ] ";" ;

assignOp     = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;
block        = "{" { statement } "}" ;
params       = "(" [ IDENT { "," IDENT } [ "," ] ] ")" ;
```

The left side of an assignment must be a variable, an index `a[i]`, or a field `a.b`. Assignment is a statement, not an expression, so `if x = 1` is a syntax error rather than a silent bug.

A `{` at the start of a statement always begins a block; in expression position it begins a map.

## Expressions

Each rule binds tighter than the one above it.

```text
expression   = or ;
or           = and { "||" and } ;
and          = equality { "&&" equality } ;
equality     = comparison { ( "==" | "!=" ) comparison } ;
comparison   = term { ( "<" | "<=" | ">" | ">=" ) term } ;
term         = factor { ( "+" | "-" ) factor } ;
factor       = unary { ( "*" | "/" | "%" ) unary } ;
unary        = ( "!" | "-" ) unary | postfix ;
postfix      = primary { "(" [ arguments ] ")" | "[" expression "]" | "." IDENT } ;
arguments    = expression { "," expression } [ "," ] ;

primary      = NUMBER | STRING | "true" | "false" | "nil" | IDENT
             | "(" expression ")"
             | "[" [ arguments ] "]"
             | "{" [ entry { "," entry } [ "," ] ] "}"
             | "fn" params block ;

entry        = ( IDENT | "[" expression "]" | expression ) ":" expression ;
```

All binary operators are left-associative: `10 - 4 - 3` is `(10 - 4) - 3`.

## Lexical structure

```text
NUMBER       = DIGIT { DIGIT | "_" } [ "." DIGIT { DIGIT | "_" } ] ;
IDENT        = ( LETTER | "_" ) { LETTER | DIGIT | "_" } ;
STRING       = '"' { CHAR | ESCAPE | "{" expression "}" } '"' ;
ESCAPE       = "\n" | "\t" | "\r" | "\0" | '\"' | "\\" | "\{" | "\}" ;
comment      = "//" { any character except newline }
             | "/*" { any character } "*/" ;
```

Letters and digits may be any Unicode letters and digits. Strings may not span lines. Keywords are `let fn return if else while for in break continue true false nil`.
