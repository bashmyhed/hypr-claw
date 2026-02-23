#!/usr/bin/env python3
"""
Fibonacci Series Generator
This script generates Fibonacci numbers up to a specified count.
"""

def fibonacci(n):
    """
    Generate Fibonacci series up to n numbers
    
    Args:
        n (int): Number of Fibonacci numbers to generate
    
    Returns:
        list: List containing Fibonacci series
    """
    if n <= 0:
        return []
    elif n == 1:
        return [0]
    elif n == 2:
        return [0, 1]
    
    fib_series = [0, 1]
    for i in range(2, n):
        next_num = fib_series[i-1] + fib_series[i-2]
        fib_series.append(next_num)
    
    return fib_series

def main():
    print("Fibonacci Series Generator")
    print("=" * 30)
    
    try:
        n = int(input("Enter the number of Fibonacci numbers to generate: "))
        
        if n < 0:
            print("Please enter a non-negative number.")
            return
        
        result = fibonacci(n)
        
        print(f"\nFirst {n} Fibonacci numbers:")
        print(result)
        
    except ValueError:
        print("Please enter a valid integer.")

if __name__ == "__main__":
    main()