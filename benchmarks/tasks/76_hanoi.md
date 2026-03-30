Write a function called `hanoi` that returns the list of moves needed to solve the Tower of Hanoi puzzle with N disks.

Requirements:
- Move N disks from peg "A" to peg "C" using peg "B" as auxiliary
- Each move is a two-element list [from-peg, to-peg]
- Return the complete list of moves in order
- The number of moves should be 2^N - 1

Test case: hanoi(1) should return [["A", "C"]]
Test case: hanoi(2) should return [["A", "B"], ["A", "C"], ["B", "C"]]

Print the result of calling the function with argument 3.
