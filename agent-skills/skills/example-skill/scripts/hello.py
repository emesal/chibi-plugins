#!/usr/bin/env python3
"""Simple hello world script for the example skill."""
import sys

def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "World"
    print(f"Hello, {name}!")
    print("This script is part of the example-skill demonstration.")

if __name__ == "__main__":
    main()
