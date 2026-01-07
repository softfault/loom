# Loom

> A statically typed, modular, object-oriented scripting language written in Rust.

Loom 是一门结合了脚本语言灵活性与静态语言安全性的编程语言，灵感来自 TOML 和 Python。它拥有类似 Python 的简洁语法，但在编译期（Analyzer 阶段）就能通过强大的类型检查系统捕获错误。

它的设计目标是提供一种现代化的脚本体验：**写的时候像脚本一样流畅，跑的时候像 Rust 一样放心。**

## 核心特性

* **模块化系统 (New!)**:
* 支持多文件项目结构。
* 支持 `use` 语句导入模块。
* 支持跨模块继承 (`[Dog : lib.Animal]`) 和类型引用。


* **强类型系统**: 支持 `int`, `float`, `bool`, `str`, `char` 等基础类型，以及强大的类型推导。
* **面向对象**:
* 支持类定义 (`[ClassName]`)。
* 支持单继承。
* 支持方法重写与多态 (Dynamic Dispatch)。


* **泛型**:
* 支持泛型类 (`Box<T>`, `List<T>`)。
* **协变支持 (Covariance)**: 允许 `Box<Dog>` 赋值给 `Box<Animal>`，符合脚本语言的直觉。


* **现代控制流**:
* `if-else` 表达式。
* `while` 循环。
* `for-in` 迭代器（支持数组、字符串遍历，以及零开销的 Range `0..100`）。


* **安全性**:
* 基于 `TableId` 的全程序符号解析。
* 完整的语义分析器，支持作用域检查、类型兼容性检查和泛型约束验证。


* **Rust 驱动**: 解释器使用 Rust 编写，内存安全且高效。

## 快速开始

### 环境要求

你需要安装 [Rust](https://www.rust-lang.org/) (Cargo)。

### 构建与运行

```bash
# 克隆仓库
git clone https://github.com/softfault/loom.git
cd loom

# 运行示例脚本
cargo run tests/hello.lm

```

## 语法示例

### 1. 模块化与跨文件继承

Loom v0.0.2 引入了完整的模块系统。

**`libs/animal_lib.lm`**:

```toml
[Animal]
name: str
make_sound = () => print("Generic Sound")

```

**`main.lm`**:

```toml
# 导入模块，创建别名
use .animal_lib as lib

# 跨模块继承：Dog 继承自 animal_lib 定义的 Animal
# 编译器会自动去 lib 中查找父类定义，并检查字段兼容性
[Dog : lib.Animal]
make_sound = () => print("Woof!")

[main()]
    # 使用导入模块的类型
    a: lib.Animal = Dog()
    a.name = "Hachiko"
    
    # 多态调用
    a.make_sound() # 输出: Woof!

```

### 2. 基础语法与类型推导

```toml
[main()]
    # 变量定义 (自动推导为 str)
    greet = "Hello, Loom!"
    print(greet)
    
    # 显式类型标注
    count: int = 42
    
    if count > 10
        print("Count is big")
    else
        print("Count is small")

```

### 3. 泛型与协变

Loom 的类型系统支持泛型协变，这意味着“一箱苹果”可以被视为“一箱水果”。

```toml
[Box<T>]
val: T 
set = (v: T) => self.val = v
get = () T => return self.val

[main()]
    # 实例化泛型
    int_box = Box<int>()
    int_box.set(100)
    
    # 泛型协变演示
    box_dog = Box<Dog>()
    box_animal: Box<Animal> = box_dog

```

### 4. 迭代器

支持多种数据类型的遍历。

```toml
[main()]
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

```

## 项目架构 

Loom 的编译器架构在 v0.0.2 进行了重构，以支持多文件分析和更严格的类型检查：

1. **SourceManager (`src/source/`)**:
* 统一管理多文件源码，提供 `FileId` 到文件路径的映射。
* 支持按需加载和缓存文件内容。


2. **Parser (`src/parser/`)**:
* 基于递归下降与 Pratt Parsing。
* **更新**: 支持 `lib.Type` 形式的成员类型解析。


3. **Analyzer (`src/analyzer/`)**:
* **核心重构**: 将符号表 Key 从 `Symbol` (字符串) 升级为 `TableId` (FileId + Symbol)。这消除了同名类在不同文件中的歧义。
* **Pass 1 (Collect)**: 收集所有文件的符号定义。
* **Pass 2 (Resolve)**: 处理继承关系，支持**跨文件父类查找**和静态字段拷贝 (Static Field Copying)。
* **Pass 3 (Check)**: 深度语义检查，利用 `TableId` 进行精确的类型兼容性验证（支持泛型协变）。


4. **Interpreter (`src/interpreter/`)**:
* 基于 AST 的 Tree-Walking 解释器。
* **更新**: 实现了**动态原型链查找 (Dynamic Prototype Lookup)**。当遇到跨模块继承时，解释器会自动在运行时环境中查找导入的模块对象，从而正确调用父类方法。



## 路线图

* [x] 基础类型与控制流
* [x] 面向对象 (类、继承、多态)
* [x] 泛型系统 (Generics & Covariance)
* [x] **模块化系统 (Modules & Imports)** (v0.0.2 Completed)
* [x] **顶层函数与定义支持** (main函数，"全局"变量)
* [x] **Analyzer 架构重构 (TableId System)** (v0.0.2 Completed)
* [ ] **闭包 (Closures) 与高阶函数**
* [ ] **标准库 (Standard Library)**: 文件 IO、系统调用
* [ ] **LSP (Language Server Protocol)**: 提供代码补全和跳转定义

## License

MIT License