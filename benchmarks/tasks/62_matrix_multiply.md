Write a function called `mat-mul` that multiplies two 2x2 matrices represented as lists of lists.

Requirements:
- Each matrix is [[a, b], [c, d]]
- Return the product matrix as a 2x2 list of lists
- Use the standard matrix multiplication formula
- Result[i][j] = sum of row i of A * column j of B

Test case: mat-mul([[1, 2], [3, 4]], [[5, 6], [7, 8]]) should return [[19, 22], [43, 50]]
Test case: mat-mul([[1, 0], [0, 1]], [[5, 6], [7, 8]]) should return [[5, 6], [7, 8]]

Print the result of calling the function with arguments [[2, 1], [0, 3]] and [[1, 4], [2, 5]].
