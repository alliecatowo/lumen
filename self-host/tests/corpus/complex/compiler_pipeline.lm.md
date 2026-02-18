# Compiler Pipeline Stages

A mini compiler pipeline demonstrating the self-host pattern:
source string -> tokens -> AST -> output.

```lumen
enum Tok
  Num(val: Int)
  Op(ch: String)
end

record Ast(op: String, left: Int, right: Int)

cell lex(src: String) -> list[Tok]
  # Simplified: just returns hardcoded tokens for "2+3"
  [Tok.Num(val: 2), Tok.Op(ch: "+"), Tok.Num(val: 3)]
end

cell parse(tokens: list[Tok]) -> Ast
  let left = match tokens[0]
    Tok.Num(val:) -> val
    _ -> 0
  end
  let op = match tokens[1]
    Tok.Op(ch:) -> ch
    _ -> "?"
  end
  let right = match tokens[2]
    Tok.Num(val:) -> val
    _ -> 0
  end
  Ast(op: op, left: left, right: right)
end

cell eval_ast(ast: Ast) -> Int
  match ast.op
    "+" -> ast.left + ast.right
    "-" -> ast.left - ast.right
    "*" -> ast.left * ast.right
    _ -> 0
  end
end

cell main() -> Int
  let tokens = lex("2+3")
  let ast = parse(tokens)
  eval_ast(ast)
end
```
