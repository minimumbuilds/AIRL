Write a function called `eval-expr` that evaluates a simple arithmetic expression tree represented as nested lists.

Requirements:
- A number is a leaf node (evaluate to itself)
- An operator node is a list: [operator, left, right] where operator is "+", "-", "*", or "/"
- Recursively evaluate left and right subtrees before applying the operator
- Use integer division for "/"

Test case: eval-expr(5) should return 5
Test case: eval-expr(["+", 2, 3]) should return 5
Test case: eval-expr(["*", ["+", 1, 2], ["-", 10, 6]]) should return 12 (3 * 4)

Print the result of calling the function with argument ["+", ["*", 2, 3], ["-", 10, 4]].
