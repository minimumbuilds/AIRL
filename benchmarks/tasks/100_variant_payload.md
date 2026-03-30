Write a function called `eval-instruction` that evaluates a simple instruction set using variant matching.

Requirements:
- Instructions are variants: (Add x y), (Mul x y), (Neg x), (Const n)
- Add computes x + y where x and y are sub-instructions
- Mul computes x * y where x and y are sub-instructions
- Neg computes -x where x is a sub-instruction
- Const returns the integer n directly
- Recursively evaluate sub-instructions before applying the operation

Test case: eval-instruction((Const 5)) should return 5
Test case: eval-instruction((Add (Const 3) (Const 4))) should return 7
Test case: eval-instruction((Mul (Add (Const 2) (Const 3)) (Const 4))) should return 20

Print the result of calling eval-instruction with (Neg (Add (Const 10) (Mul (Const 3) (Const 5)))).
