Write a function called `balanced-parens` that checks whether a string has balanced parentheses.

Requirements:
- Only consider '(' and ')' characters, ignore everything else
- Return true if every '(' has a matching ')' and they are properly nested
- An empty string is balanced
- A string with no parentheses is balanced

Test case: balanced-parens("(())") should return true
Test case: balanced-parens("((())") should return false
Test case: balanced-parens(")(") should return false
Test case: balanced-parens("hello (world)") should return true

Print the result of calling the function with argument "(a (b) (c (d)))".
