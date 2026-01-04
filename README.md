# Loom

> A statically typed, object-oriented scripting language written in Rust.

Loom 是一门结合了脚本语言灵活性与静态语言安全性的编程语言,灵感来自Toml和Python。它拥有类似 Python 的简洁语法，但在编译期（Analyzer 阶段）就能通过强大的类型检查系统捕获错误。

它的设计目标是提供一种现代化的脚本体验：**写的时候像脚本一样流畅，跑的时候像 Rust 一样放心。**

## ✨ 特性 

* **强类型系统**: 支持 `int`, `float`, `bool`, `str`, `char` 等基础类型，以及强大的类型推导。
* **面向对象**:
* 支持类定义 (`[ClassName]`)。
* 支持单继承 (`[Dog : Animal]`)。
* 支持方法重写与多态 (Dynamic Dispatch)。


* **泛型**:
* 支持泛型类 (`Box<T>`, `List<T>`)。
* **协变支持 (Covariance)**: 允许 `Box<Dog>` 赋值给 `Box<Animal>`，符合脚本语言的直觉。


* **现代控制流**:
* `if-else` 表达式。
* `while` 循环。
* `for-in` 迭代器（支持数组、字符串遍历，以及零开销的 Range `0..100`）。


* **安全性**: 完整的语义分析器，支持作用域检查、类型兼容性检查和泛型约束验证。
* **Rust 驱动**: 解释器使用 Rust 编写，内存安全且高效。

## 🚀 快速开始 

### 环境要求

你需要安装 [Rust](https://www.rust-lang.org/) (Cargo)。

### 构建与运行

```bash
# 克隆仓库
git clone https://github.com/your-username/loom.git
cd loom

# 运行示例脚本
cargo run tests/hello.lm

```

## 📖 语法示例 

### 1. 基础语法与类型推导

```toml
[Main]
main = () int
    # 变量定义 (自动推导为 str)
    greet = "Hello, Loom!"
    print(greet)
    
    # 显式类型标注
    count: int = 42
    
    if count > 10
        print("Count is big")
    else
        print("Count is small")
        
    return 0

```

### 2. 面向对象与多态

Loom 支持完整的类继承体系和运行时多态。

```toml
[Animal]
make_sound = () => print("...")

[Dog : Animal]
make_sound = () => print("Woof!")

[Cat : Animal]
make_sound = () => print("Meow!")

[Trainer]
train = (a: Animal) 
    print("Training session start:")
    a.make_sound() # 动态分派：根据运行时类型调用正确的方法

[Main]
main = () int
    trainer = Trainer()
    
    d = Dog()
    c = Cat()
    
    trainer.train(d) # 输出: Woof!
    trainer.train(c) # 输出: Meow!
    
    return 0

```

### 3. 泛型与协变 

Loom 的类型系统支持泛型协变，这意味着“一箱苹果”可以被视为“一箱水果”。

```toml
[Box<T>]
val: T 
set = (v: T) => self.val = v
get = () T => return self.val

[Main]
main = () int
    # 实例化泛型
    int_box = Box<int>()
    int_box.set(100)
    
    # 泛型协变演示
    box_dog = Box<Dog>()
    box_animal: Box<Animal> = box_dog
    
    return 0

```

### 4. 迭代器 

支持多种数据类型的遍历。

```toml
[Main]
main = () int
    # 1. 数组遍历
    arr = [10, 20, 30]
    for x in arr
        print(x)

    # 2. 字符串遍历
    str = "Loom"
    for c in str
        print(c) # L, o, o, m

    # 3. Range 遍历 (Lazy Evaluation)
    # 不会分配内存，直接生成数值
    for i in 0..5
        print(i) # 0, 1, 2, 3, 4
        
    return 0

```

## 🛠️ 项目架构 

Loom 的编译器架构清晰，分为三个主要阶段：

1. **Parser (`src/parser/`)**:
* 基于递归下降 (Recursive Descent) 算法。
* 支持优先级解析 (Pratt Parsing) 处理表达式。
* 生成类型安全的 AST (`src/ast.rs`).


2. **Analyzer (`src/analyzer/`)**:
* **Pass 1 (Collect)**: 扫描所有文件，收集类和方法的符号定义。
* **Pass 2 (Resolve)**: 解析类型引用，建立继承关系图。
* **Pass 3 (Check)**: 深度语义检查。
* `check/expr.rs`: 表达式类型检查。
* `check/stmt.rs`: 控制流与作用域检查。
* `check/decl.rs`: 泛型约束、方法重写兼容性检查。


* 实现了复杂的类型兼容性逻辑（包括协变）。


3. **Interpreter (`src/interpreter/`)**:
* 基于 AST 的 Tree-Walking 解释器。
* 使用 `Rc<RefCell<Environment>>` 管理运行时作用域和闭包环境。
* 内置值类型 (`Value`) 支持引用计数管理的对象模型。



## 🗺️ 路线图 (Roadmap)

* [x] 基础类型与控制流
* [x] 面向对象 (类、继承、多态)
* [x] 泛型系统 (Generics & Covariance)
* [x] 模块化分析器架构重构
* [ ] **闭包 (Closures) 与高阶函数**
* [ ] **标准库 (Standard Library)**: 文件 IO、系统调用
* [ ] **编译后端 (AOT) 或jit**

## License

MIT License