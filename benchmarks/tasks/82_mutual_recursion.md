Write two mutually recursive functions: `my-even?` and `my-odd?` that determine if a non-negative integer is even or odd without using modulo.

Requirements:
- my-even?(0) returns true, my-odd?(0) returns false
- my-even?(n) calls my-odd?(n-1)
- my-odd?(n) calls my-even?(n-1)
- Both functions require non-negative input
- Do NOT use modulo (%) — this tests mutual recursion

Test case: my-even?(4) should return true
Test case: my-odd?(5) should return true
Test case: my-even?(3) should return false

Print the result of calling my-even? with argument 7.
