<div align="center">
  <img src="logo.svg" width="120" alt="Loom Logo" />
  <h1>Loom</h1>
  <p>
    <strong>A general-purpose, statically typed programming language.</strong>
  </p>
  <p>
    Structure of Rust · Fluidity of Python · Modular by Design
  </p>
</div>

---

> **Design Goal:** To engineer the gap between high-level prototyping and low-level system programming.

**Loom** is a strictly typed, object-oriented language built for reliability and scale. Inspired by the robustness of **Rust** and the readability of **Python**, it features a clean, indentation-sensitive syntax but enforces correctness at compile-time through a rigorous Analyzer and Type System.

Currently powered by a memory-safe interpreter written in Rust, Loom is evolving towards a fully compiled system language capable of self-hosting and low-level development.

## What is Loom?

* **Statically Typed**: No implicit type guessing. Errors are caught during the Analysis phase, not in production.
* **Modular**: Enforces a strict, modern module system ("Rust 2018+" style) for scalable project architecture.
* **System-Oriented Vision**: While currently interpreted, Loom's semantics (RTTI, strict scoping, explicit types) are designed with a future AOT compiler and OS development in mind.

**Philosophy**: *Write with the fluidity of a script, run with the confidence of a system language.*

## What's New in v0.0.4

* **Syntax Overhaul**: Moved from TOML-style `[...]` headers to standard `class` and `fn` keywords.
* **Modern Module System**: Enforced strict, "Rust 2018+" style module resolution (no `mod.lm` or `init.lm` files).
* **VS Code Support**: Official extension published on Open VSX.
* **Scoping Fixes**: Correct lexical scoping for cross-module inheritance and field initialization.

## Core Features

* **Modular System**:
* Supports multi-file project structures using **"Natural Extension"** style (e.g., `utils.lm` alongside `utils/`).
* Supports the `use` statement for module imports (e.g., `use std.io`).
* Supports cross-module inheritance (`class Dog : lib.Animal`) and type referencing.


* **Strong Type System**:
* Basic types: `int`, `float`, `bool`, `str`, `char`, `any`.
* **Strict Typing**: Explicit type annotations required for fields and function signatures.
* **RTTI**: Runtime Type Information supporting safe downcasting (`as` operator).


* **Object-Oriented**:
* Class definitions (`class Name`).
* Single inheritance.
* Method overriding and Dynamic Dispatch.


* **Generics**:
* Generic classes (`class Box<T>`).
* **Covariance**: Allows assigning `Box<Dog>` to `Box<Animal>`.


* **Modern Control Flow**:
* Expression-oriented design (almost everything is an expression).
* `if-else`, `while`, `for-in` iterators.
* Zero-overhead Ranges (`0..100`).


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
cargo run example/hello.lm

```

### IDE Support

Loom has a VSCodium/VS Code extension providing syntax highlighting and snippets.
Search for **Loom** in the Open VSX Registry or install manually from the `extension/` folder.

## Syntax Examples

### 1. Modularity & Cross-File Inheritance

Loom v0.0.3 enforces a clean module structure.

**`libs/animal_lib.lm`**:

```loom
class Animal 
    name: str
    fn make_sound()
        print("Generic Sound")

```

**`main.lm`**:

```loom
// Import module and create an alias
use libs.animal_lib as lib

// Cross-module inheritance: Dog inherits from Animal defined in animal_lib
class Dog : lib.Animal 
    fn make_sound() 
        print("Woof!")

fn main() 
    // Use types from the imported module
    // Variables are defined with 'name: Type = value'
    a: lib.Animal = Dog()
    a.name = "Hachiko"
    
    // Polymorphic call (Dynamic Dispatch)
    a.make_sound() // Output: Woof!

```

### 2. Basic Syntax & Flow Control

Loom uses Python-style indentation for blocks, but C-style comments (`//`).

```loom
// Top-level variable
global_conf: str = "Production"

fn main() 
    // Variable definition
    greet: str = "Hello, Loom!"
    print(greet)
    
    count: int = 42
    
    if count > 10 
        print("Count is big")
     else 
        print("Count is small")

```

### 3. Generics & Covariance

Loom's type system supports generic covariance, meaning "a box of apples" can be treated as "a box of fruit" (read-only context safety).

```loom
class Box<T> 
    val: T 
    
    fn set(v: T) 
        self.val = v
     
    fn get() T 
        return self.val

fn main() 
    // Instantiate generic
    int_box: Box<int> = Box<int>()
    int_box.set(100)
    
    // Generic covariance demonstration
    // Assuming Dog inherits from Animal
    box_dog: Box<Dog> = Box<Dog>()
    box_animal: Box<Animal> = box_dog 

```

### 4. Arrays & Iterators

```loom
fn main() 
    // 1. Array Literal
    arr: [int] = [10, 20, 30]
    
    // 2. For-in Loop
    for x in arr 
        print(x)

    // 3. Range traversal (Lazy Evaluation)
    // Does not allocate memory, generates values directly
    for i in 0..5 
        print(i) // 0, 1, 2, 3, 4

```

## Project Architecture

The Loom compiler utilizes a multi-pass architecture refactored for correct scoping and modularity:

1. **SourceManager (`src/source/`)**:
* Manages `FileId` mapping and on-demand file loading.


2. **Parser (`src/parser/`)**:
* **Refactor (v0.0.3)**: Migrated from Bracket-style to Keyword-style (`class`/`fn`).
* Uses **Pratt Parsing** for expressions to handle precedence and associativity correctly.


3. **Analyzer (`src/analyzer/`)**:
* **Pass 1 (Collect)**: Scans all files for type definitions (`TableId`).
* **Pass 2 (Resolve)**: Resolves inheritance hierarchy and copies fields from parent classes.
* **Pass 3 (Check)**: Performs deep semantic analysis, type checking, and scope verification.


4. **Interpreter (`src/interpreter/`)**:
* **Lexical Scoping Fix**: When instantiating classes from imported modules, the interpreter correctly switches execution context to the **defining module's environment**. This ensures private globals and dependencies in libraries work as expected.



## Roadmap

* [x] **Syntax Overhaul (v0.0.3)**: `class` / `fn` keywords, C-style comments.
* [x] **Modular System**: "Modern Rust" style resolution (No `init.lm`).
* [x] **Object-Oriented**: Classes, Inheritance, Polymorphism.
* [x] **Generics**: Basic Generics & Covariance.
* [x] **VS Code Extension**: Syntax highlighting and snippets.
* [ ] **Closures**: Anonymous functions and environment capturing.
* [ ] **Standard Library**: File I/O, System Calls, Math Lib.
* [ ] **LSP (Language Server Protocol)**: Code completion and Go-to-Definition.

## License

MIT License