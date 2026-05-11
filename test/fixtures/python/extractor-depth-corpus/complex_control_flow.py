"""Complex control flow fixture for PY-EXT-2 validation.

This file contains functions with high cyclomatic complexity
to validate metrics extraction.
"""

import os
from typing import Optional, List


def highly_complex_function(data: List[int], threshold: int) -> Optional[int]:
    """Function with cyclomatic complexity >= 8.

    Decision points:
    - if data (1)
    - for loop (1)
    - if item > threshold (1)
    - elif item < 0 (1)
    - and condition (1)
    - while loop (1)
    - try/except (1)
    Base: 1
    Total: 8+
    """
    result = 0

    if data:
        for item in data:
            if item > threshold:
                result += item
            elif item < 0 and threshold > 0:
                result -= item
            else:
                result += 1

        index = 0
        while index < len(data):
            try:
                result += data[index]
            except IndexError:
                break
            index += 1

    return result if result > 0 else None


def deeply_nested_function(matrix: List[List[int]]) -> int:
    """Function with deep nesting (depth >= 4).

    Nesting levels:
    - if matrix (1)
    - for row (2)
    - for cell (3)
    - if cell > 0 (4)
    - while loop inside (5)
    """
    total = 0

    if matrix:
        for row in matrix:
            for cell in row:
                if cell > 0:
                    temp = cell
                    while temp > 0:
                        total += 1
                        temp -= 1

    return total


def exception_heavy_function(value: int) -> str:
    """Function with multiple exception handlers."""
    result = ""

    try:
        if value > 100:
            raise ValueError("Too large")
        elif value < 0:
            raise TypeError("Negative not allowed")
        result = str(value)
    except ValueError:
        result = "value_error"
    except TypeError:
        result = "type_error"
    except Exception:
        result = "unknown_error"

    return result


def with_statement_complexity(paths: List[str]) -> List[str]:
    """Function using context managers."""
    contents = []

    for path in paths:
        if os.path.exists(path):
            with open(path) as f:
                data = f.read()
                if data:
                    contents.append(data)

    return contents


def boolean_complexity(a: bool, b: bool, c: bool, d: bool) -> bool:
    """Function with boolean operator complexity."""
    # Each 'and' and 'or' is a decision point
    if a and b:
        return True
    elif c or d:
        if a or c:
            return b and d
        return False
    return a and b and c and d
