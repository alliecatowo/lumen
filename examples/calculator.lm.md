# Calculator

A command-line calculator demonstrating pattern matching, recursion, and type safety.

This example shows how Lumen's match expressions enable clean handling of different operations,
while the type system ensures correctness at compile time.

```lumen
cell add(a: int, b: int) -> int
    a + b
end

cell subtract(a: int, b: int) -> int
    a - b
end

cell multiply(a: int, b: int) -> int
    a * b
end

cell safe_divide(a: int, b: int) -> int
    if b == 0
        print("Error: Division by zero")
        0
    else
        a / b
    end
end

cell power(base: int, exp: int) -> int
    if exp == 0
        1
    else
        if exp < 0
            0  # Integer power doesn't support negative exponents
        else
            base * power(base, exp - 1)
        end
    end
end

enum Result
    Success(int)
    Error(string)
end

cell calculate_with_check(a: int, b: int, op: string) -> Result
    match op
        "add" -> Success(add(a, b))
        "sub" -> Success(subtract(a, b))
        "mul" -> Success(multiply(a, b))
        "div" ->
            if b == 0
                Error("Division by zero")
            else
                Success(safe_divide(a, b))
            end
        "pow" -> Success(power(a, b))
        _ -> Error("Unknown operation")
    end
end

cell print_result(res: Result)
    match res
        Success(val) -> print("Result: {val}")
        Error(msg) -> print("Error: {msg}")
    end
end

cell main()
    print("=== Calculator Demo ===")
    print("")

    print("15 + 7 = {add(15, 7)}")
    print("20 - 8 = {subtract(20, 8)}")
    print("6 × 7 = {multiply(6, 7)}")
    print("56 ÷ 8 = {safe_divide(56, 8)}")
    print("2^10 = {power(2, 10)}")

    print("")
    print("Using Result type with error handling:")
    let res1 = calculate_with_check(10, 5, "add")
    print_result(res1)

    let res2 = calculate_with_check(10, 0, "div")
    print_result(res2)

    let res3 = calculate_with_check(2, 8, "pow")
    print_result(res3)

    let res4 = calculate_with_check(5, 3, "invalid")
    print_result(res4)
end
```
