Write a function called `sliding-window-sum` that takes a list of integers and a window size, returning a list of sums for each window position.

Requirements:
- Each element in the result is the sum of a contiguous window of the given size
- The result has (length - window_size + 1) elements
- Window size must be at least 1 and at most the list length
- If the list is shorter than the window, return an empty list

Test case: sliding-window-sum([1, 2, 3, 4, 5], 3) should return [6, 9, 12]
Test case: sliding-window-sum([10, 20, 30], 1) should return [10, 20, 30]

Print the result of calling the function with arguments [1, 3, 5, 7, 9, 11] and 3.
