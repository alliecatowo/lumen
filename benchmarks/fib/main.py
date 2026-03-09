#!/usr/bin/env python3
"""Fibonacci benchmark: recursive fib(35) = 9227465"""
import sys
sys.setrecursionlimit(100000)


def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)


print(fib(35))
