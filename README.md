# Loom

> A statically typed, modular, object-oriented scripting language written in Rust.

Loom is a programming language that blends the flexibility of scripting languages with the safety of static typing, inspired by TOML and Python. It features a clean, Python-like syntax but captures errors at compile-time (Analyzer phase) through a robust type-checking system.

Its design goal is to provide a modern scripting experience: **Write with the fluidity of a script, run with the confidence of Rust.**

## Core Features

* **Modular System (New!)**:
* Supports multi-file project structures.
* Supports the `use` statement for module imports.
* Supports cross-module inheritance (`[Dog : lib.Animal]`) and type referencing.


* **Strong Type System**: Supports basic types like `int`, `float`, `bool`, `str`, `char`, along with powerful type inference.
* **Object-Oriented**:
* Supports class definitions (`[ClassName]`).
* Supports single inheritance.
* Supports method overriding and Dynamic Dispatch.


* **Generics**:
* Supports generic classes (`Box<T>`, `List<T>`).
* **Covariance Support**: Allows assigning `Box<Dog>` to `Box<Animal>`, aligning with scripting language intuition.


* **Modern Control Flow**:
* `if-else` expressions.
* `while` loops.
* `for-in` iterators (supports array and string traversal, as well as zero-overhead Ranges `0..100`).


* **Safety**:
* Whole-program symbol resolution based on `TableId`.
* Complete semantic analyzer supporting scope checks, type compatibility checks, and generic constraint verification.


* **Rust-Powered**: The interpreter is written in Rust, ensuring memory safety and high efficiency.

## Quick Start

### Prerequisites

You need to have [Rust](https://www.rust-lang.org/) (Cargo) installed.

### Build & Run

```bash
# Clone the repository
git clone https://github.com/softfault/loom.git
cd loom

# Run the example script
cargo run tests/hello.lm

```

## Syntax Examples

### 1. Modularity & Cross-File Inheritance

Loom v0.0.2 introduces a complete module system.

**`libs/animal_lib.lm`**:

```loom
[Animal]
name: str
make_sound = () => print("Generic Sound")

```

**`main.lm`**:

```loom
# Import module and create an alias
use .animal_lib as lib

# Cross-module inheritance: Dog inherits from Animal defined in animal_lib
# The compiler automatically looks up the parent definition in lib and checks field compatibility
[Dog : lib.Animal]
make_sound = () => print("Woof!")

[main()]
    # Use types from the imported module
    a: lib.Animal = Dog()
    a.name = "Hachiko"
    
    # Polymorphic call
    a.make_sound() # Output: Woof!

```

### 2. Basic Syntax & Type Inference

```loom
[main()]
    # Variable definition (automatically inferred as str)
    greet = "Hello, Loom!"
    print(greet)
    
    # Explicit type annotation
    count: int = 42
    
    if count > 10
        print("Count is big")
    else
        print("Count is small")

```

### 3. Generics & Covariance

Loom's type system supports generic covariance, meaning "a box of apples" can be treated as "a box of fruit".

```loom
[Box<T>]
val: T 
set = (v: T) => self.val = v
get = () T => return self.val

[main()]
    # Instantiate generic
    int_box = Box<int>()
    int_box.set(100)
    
    # Generic covariance demonstration
    box_dog = Box<Dog>()
    box_animal: Box<Animal> = box_dog

```

### 4. Iterators

Supports traversal of various data types.

```loom
[main()]
    # 1. Array traversal
    arr = [10, 20, 30]
    for x in arr
        print(x)

    # 2. String traversal
    str = "Loom"
    for c in str
        print(c) # L, o, o, m

    # 3. Range traversal (Lazy Evaluation)
    # Does not allocate memory, generates values directly
    for i in 0..5
        print(i) # 0, 1, 2, 3, 4

```

## Project Architecture

The Loom compiler architecture was refactored in v0.0.2 to support multi-file analysis and stricter type checking:

1. **SourceManager (`src/source/`)**:
* Unifies management of multi-file source code, providing a mapping from `FileId` to file paths.
* Supports on-demand loading and content caching.


2. **Parser (`src/parser/`)**:
* Based on Recursive Descent and Pratt Parsing.
* **Update**: Supports parsing of member types in the format `lib.Type`.


3. **Analyzer (`src/analyzer/`)**:
* **Core Refactor**: Upgraded the Symbol Table Key from `Symbol` (String) to `TableId` (FileId + Symbol). This eliminates ambiguity for classes with the same name in different files.
* **Pass 1 (Collect)**: Collects symbol definitions from all files.
* **Pass 2 (Resolve)**: Handles inheritance relationships, supporting **cross-file parent lookup** and Static Field Copying.
* **Pass 3 (Check)**: Deep semantic checking, utilizing `TableId` for precise type compatibility verification (supporting generic covariance).


4. **Interpreter (`src/interpreter/`)**:
* AST-based Tree-Walking interpreter.
* **Update**: Implemented **Dynamic Prototype Lookup**. When encountering cross-module inheritance, the interpreter automatically looks up imported module objects in the runtime environment to correctly invoke parent methods.



## Roadmap

* [x] Basic Types & Control Flow
* [x] Object-Oriented (Classes, Inheritance, Polymorphism)
* [x] Generics System (Generics & Covariance)
* [x] **Modular System (Modules & Imports)** (v0.0.2 Completed)
* [x] **Top-level Functions & Definitions** (main function, "global" variables)
* [x] **Analyzer Architecture Refactor (TableId System)** (v0.0.2 Completed)
* [ ] **Closures & Higher-Order Functions**
* [ ] **Standard Library**: File I/O, System Calls
* [ ] **LSP (Language Server Protocol)**: Code completion and Go-to-Definition

## License

MIT License