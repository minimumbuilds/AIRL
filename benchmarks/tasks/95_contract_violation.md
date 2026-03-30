Write a function called `checked-sqrt` that computes the integer square root with a contract that rejects negative inputs.

Requirements:
- The :requires contract must enforce (>= n 0)
- The function should compute the floor square root
- The :ensures contract must enforce (>= result 0)
- Call the function with a NEGATIVE number (-4) inside a match on the Result of string-to-int to demonstrate error handling

Write the function, then demonstrate calling it with -4. Since the contract will fail, wrap the call in error handling that catches the violation and prints "Contract violated: negative input". For the positive case, call with 16 and print the result.

Print the result of calling checked-sqrt with 25.
