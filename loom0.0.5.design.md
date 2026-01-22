## 前言与概述

### 简介

*The Loom Programming Language*

目前在`0.0.4`，还是一个解释型。将在`0.0.5`正式彻底重构成为编译型语言。`Loom`这个名字来源于`Lmot`，既`Toml`的倒写。最开始的`Loom`是一个类似于`TomlScript`的完全兼容`Toml`的脚本语言，用于在`Toml`的基础上构建一个保持兼容和设计理念，但是能够做一些简单的逻辑和计算的构建脚本。最开始的`Loom`设计很简单，支持泛型，类，对象（在`0.0.3`及之前都叫做`表对象`），当然还有函数。数据类型也只有`int`, `float`, `str`和`[T]`（数组）。

> 几乎是一个下午设计出来的，比`JavaScript`还要吓人...

但是其实到现在我都还是感觉设计的不错，如果没有之后的进展的话，也许我还会继续向 *"又一个构建脚本"* 的方向发展（同类的有xmake, cmake等，其中有些用的是另外一个语言，有些则是自己自创了一种dsl）。然而`Loom`先天就带有了一种使命——接替`Nama`的位置。`Loom`的确最早是作为`Nama`这门实验语言的构建脚本提出的，但是由于我在编写`Loom`解释器的时候大量使用了和`Nama`高度重叠的代码，导致这门解释型语言天生就带有了泛型和严格的静态类型。没错，即使他是作为一个很随意的使用情节提出的...（所以甚至打印字符串的机制都是对`str`类型和另外一个泛型重载`+`号）

```early loom
[WorkSpace]
name = "Loom-Project"
version = [0, 1, 0]

[main()]
    my_space = WorkSpace()
    print(my_space.name + my_space.version)
```

当然了，你也可以在某个变量后面标注类型，就像是高版本的`Python3`一样。虽然现在的`Loom`设计大不相同，但是其中的一个哲学一直保持了下来——**Data shapes behavior. (数据定义行为)**。或者说，*配置式设计*，就像`Toml`或者`Json`一样，直观简洁。

`Nama`项目其实完成的很好了，已经是基本能够编写大型项目的程度，作为一个脱胎于`Ninelang`的语言已经算是完成了他的历史使命。然而我还是需要一个语言，能够用于编写复杂的VideoGames,能够编写操作系统内核，同时还有这个操作系统的软件和GUI。总结一下，就是有**能够制作一个完全独立的GUI操作系统的能力**的语言。那么这门语言一定是有各种`features`的。如果管理不好，就会变成`C++`，臃肿且杂乱。`Loom`有编织的意思，我希望我能设计一种正交的语言，在保持简洁的同时保持优雅，而不是过渡追求某种片面。`Nama`主要是语法太逆天了，连我这个作者都不愿意写，且本身编译器架构非常乱，不好维护，毕竟是第一个“能跑”的项目。

这便是`Loom`的由来。

### 起步示例

任何语言都可以先展示他的 Hello, World:

```loom
use std.debug.println;

pub fn main() {
    println("hello, world!");
}
```

其中，`use`用于导入模块。这里不是`use .std.debug.println;`是因为我们要导入绝对位置。如果有`.`或者`..`则是表达当前目录下或者向上索引。在`Loom`中，一个文件夹天然形成一个模块，而模块和文件系统是耦合的（当然我指的是类UNIX文件系统），同时也和命名空间耦合。这种设计是为了直观和方便。而println则是一个宏。在`Loom`中，宏也受到模块的约束（可以理解为命名空间下的宏，但是实现机制是载入+检查，配合多重扫描的乱序定义）

当然，一些语言喜欢用斐波那契数列来展现语言的算法编写感觉:

```loom
fn fib(n: i32) i32 {
    if n <= 1 {
        return n;
    }

    fib(n - 1) + fib(n - 2)
}

fn main() {
    let result = fib(10);
    std.debug.println("Fib(10) is: {}", result);
}
```

接触过`Rust`的话对此一定不会陌生。`fib(n - 1) + fib(n - 2)`这一行是表达式驱动，而`let`则是声明了一个不可变的变量。其他的都差不多，都是一个现代语言该有的样子。

此外，为了展现 Loom 处理实际数据的能力，可以看这个简单的任务列表处理。这展示了如何定义自定义的纯粹数据结构（Plain Struct）以及如何遍历集合：

```loom
// 定义一个数据结构
struct Task {
    id: i32,
    name: []char,
    completed: bool,
}

pub fn main() {
    // 声明一个包含结构体的数组
    let tasks = [
        Task { id: 1, name: "编写语言规范", completed: true },
        Task { id: 2, name: "实现词法分析器", completed: false },
        Task { id: 3, name: "设计类型系统", completed: false },
    ];

    // 遍历并处理
    for var i = 0; i < #tasks; i += 1 {
        let task = &tasks[i]; 
        if !task.completed {
            std.debug.println("待办事项 [{}]: {}", task.id, task.name);
        }
    }
}
```

### 执行模型

就是朴素的AOT。入口点则是main()函数，需要pub因为要被标准Loom上下文调用。

## 词法结构

#### 源文件编码

以 UTF-8 编码

#### 空白与换行执行

类`Rust`风格。
* 语言是*表达式驱动*的。
* 分号 ; 用于分隔语句，并抑制表达式的返回值（返回unit）。
* 换行符通常被视为分界符，但在未完成的表达式中会被忽略。

#### 注释

* 单行注释`//`
* 多行/嵌入式注释`/**/`（支持嵌套）

#### 标识符

[a-zA-Z][a-zA-Z0-9_]*

关键字列表：

声明： `fn`, `let`, `var`, `const`, `enum`, `union`, `class`, `struct`, `use`, `impl`, `trait`, `type`, `macro`
控制流： `if`, `else`, `match`, `for`, `in`, `break`, `continue`, `return`,
可见性： `pub`,`extern`
逻辑/值： `true`, `false`, `undef`
特殊：`self`, `Self`, `as`, `defer`

#### 字面量

Loom 支持整数、浮点数、布尔值、字符和字符串字面量。

##### 1. 整数

整数文本可以采用十进制、十六进制、八进制或二进制表示。为了提高可读性，整数文本中允许使用下划线 _ 作为分隔符，编译器会在处理时忽略这些下划线。
* 十进制 (Decimal): 默认形式。例如：98, 12_345。
* 十六进制 (Hexadecimal): 以 0x 开头。例如：0xFF, 0xdead_beef。
* 八进制 (Octal): 以 0o 开头。这在处理文件权限时非常有用。例如：0o777, 0o755。
* 二进制 (Binary): 以 0b 开头。常用于位掩码。例如：0b1111_0000, 0b1010。

未标注类型的整数字面量默认推导为`usize`（平台相关字长），以方便系统编程与索引操作。

##### 2. 浮点数

浮点数字面量包含一个小数部分、一个指数部分，或两者兼有。

* 小数形式: 3.14, 0.5, -10.0。
* 科学计数法: 使用 e 或 E 表示指数。例如：2.5e-3 (即 0.0025), 1E6。

未标注类型的浮点数字面量默认推导为`f32`

##### 3. 布尔值

仅有两个保留关键字用于表示布尔值：`true`, `false`

##### 4. 字符与字符串

Loom 区分字符（单个 Unicode 标量值）和字符串（UTF-8 字节序列）。

1. 字符 (char): 使用单引号 ' 包裹。支持转义序列。

* 示例: 'a', '中', '\n', '\u{1F600}'。
* 类型: char (通常是 4 字节，代表 Unicode Scalar Value), 或者u8,取决于指定类型。默认推导为u8,编译器会智能处理一些需要推导为char的情况。
```loom
let a = 'a';      // 推导为 u8 (0x61)
let b: char = 'b';// 显式指定为 char
let word = '好';   // 推导为 char (U+597D)
```

1. 字符串 (String): 使用双引号 " 包裹，支持标准转义序列（如 \n, \t）。

* 示例: "hello", "Loom Language", "Line 1\nLine 2"。
* 类型: 默认为`[]char`或`[]u8`。字符串是一种语法糖，会因为`Loom`的静态数组提升机制而被放在.data或者.bss字段，并得到其切片。（具体机制会在后续的数组一栏中介绍，这里只说明默认不标注类型情况）

##### 5. 未定义值

`undef` 关键字仅用于表示**未初始化的内存**。它不是“空”，而是“垃圾数据”。

* `undef` 主要用于性能敏感场景（如分配大数组但不希望付出清零的开销）。
* `undef` **不能参与任何运算**，包括比较运算（`==`, `!=`）。
* 读取 `undef` 变量是未定义行为。变量必须在被写入（初始化）后才能读取。

```loom
// 栈上分配 1024 字节，不执行 memset 0，极快
var buffer: [1024]u8 = undef; 

// 必须先写入
fill_buffer(&buffer);

// 之后才能读取
process(buffer[0]);

```

## 类型系统

### 基本类型

1. 整数

任意位宽的i和u。比如，`i4`, `u9`等。最高分别到`i128`和`u128`。
当然还有`usize`和`isize`两个平台相关的位宽类型。

2. 浮点数

`f16`, `f32`, `f64`, `f128`。

3. 字符

`char`，表示Unicode，`u8`用于表示传统Anscii字符.

4. 未定义

`undef`

### 复合类型

1. 数组

```loom
let array: [2][4]i32 = [[1, 2, 3, 4], [5, 6, 7, 8]];
```
数组默认分配在栈上。

2. 切片

```loom
let slice: []u8 = "hello, loom!";
let string = "holiday"; // 字符串默认为切片
let array = [0; 1024]; // 数组同样默认推导为切片
let len = #array; // #A 为获取长度的语法，理解为“取长度”
```

*数组默认提升原则*:

当你写`let array = [0; 1024];`时，这个1024长度的数组首先会因为整数默认推导规则被推导为`usize`的数组，然后再被放在该文件最终编译产物的`.data`字段，相当于定义了一个全局变量，然后最后你得到的将是一个切片而不是数组。这是 Loom 中数组的默认行为。
当然了，你可以通过类型标注指定你想要的行为。
```loom
let string = "hello!"; // []u8, 全局变量
let buffer: [1024]u8 = [0; 1024]; // 指定出来的栈上数组
let str: [_]u8 = "hello!\0" // 甚至可以写出来栈上字符串，只要你想的话
```

3. 元组

```loom
let rgb: (u8, u8, u8) = (2, 4, 202);
std.debug.println("{}", rgb[0]);

var result: ([]char, i32) = (undef, 10);
result[0] = "hello";
std.debug.println("{}", result[0]);
```

元组本质是一个匿名的结构体。

4. 枚举

```loom
pub enum Message: i32 {
    Quit = 3,
    Move {
        x: i32, 
        y: i32,
    },
    Write([]char),
    pub ChangeColor(i32, i32, i32),
}

let msg = Message.Move{x: 10, y: 20};
let text_msg = Message.Write("hello");

fn consume(msg: Message) {
    match msg {
        .Quit => println("I quit game"),
        .Move{x, y} => println("move to ({}, {})", x, y),
        else => (),
    };
}
```

Loom的枚举是ADT。`.Quit`意为类型推导，写全就是`Message.Quit`。这里由于在match上下文中，所以能够省略类型，是一种常用的简写方式。

5. 朴素结构体

```loom
struct Point {
    x: f32,
    y: f32 = 0, // 默认值
}

let p = Point{x:10, y: 4.8};
let x = p.x;
var p1 = Point{ x: 9.4, y: 1};
p1.x = 0;
let p2 = Point{x: 1}; 
```

可见性展示
```loom
pub struct Point {
    pub x: f32,
    pub y: f32
}
```

1. 特征

`trait`用于描述某种类型的行为，可以理解为类型的类型。具体的一些介绍在下面的泛型中讲述。`trait`是 Loom 类型系统的核心，它与内存布局（struct/class）完全正交。主要用于描述泛型参数的边界和能力。就像函数参数必须指定类型才能知道需要在栈上预留多大的空间一样，在泛型函数中，泛型参数必须指定`trait`才能让编译器知道该类型具备什么行为（以及潜在的大小约束）。
```loom
// T 不是一个“空”占位符，它被 Addable 约束
// 这告诉编译器：T 是可以执行加法操作的类型
fn add(T: Addable, a: T, b: T) T {
    a + b
}
```
`trait`是非侵入式的，不改变对象的内存布局，它只存在于编译期的类型检查阶段，除非你用`as`向编译器申请“我要使用一个动态的`trait`对象”。
```loom
let a: i32 = 10;
let addable: Addable = a as Addable;
```

定义`trait`的一般语法

```loom
trait Addable {
    fn add(self, other: Self) Self;
}

impl i32: Addable {
    fn add(self, other: i32) i32 {
        self + other
    }
}
```

1. 指针

指针是Loom的核心。当然，一切高级语言都应该以指针作为核心。Loom的其他特性都是可有可无的，唯独不能没有指针。

```loom
let a: i32 = 10;
let ptr_to_a: &i32 = &a;
let b: i32 = ptr_to_a.&;

var c = 1000;
let ptr = &mut c;
ptr.& += 1;
println("{}", c); // 1001
```

指针可以参与运算。
```loom
let array = [1, 2, 3]; // []usize
var ptr = &array[0];
println("{}", (ptr + 1).&); // 2
```

与切片互操作
```loom
let array: []usize = [1, 2, 3];
let ptr = &array[0]; // 从切片获取指针
let array1 = ptr[0..3]; // 从指针获取切片。注意这里需要仔细检查是否安全。
```

与一些常见的语言的语法不同，在Loom 中指针统一用`&`符号表示，注意不要和某些语言中的*引用*混淆。Loom中没有引用这种设计。指针就是一个地址而已。

在Loom中还有一类特殊的指针，用于表达`volantile`的语义,使用`*`来表示，用于编写驱动。 `*mut`则是指向可变的数据。

**指针与判空 (Pointers & Null)**

Loom 没有 `null` 关键字。空指针在底层仅仅是地址为 `0` 的指针。这主要用于与 C 语言或操作系统 API 交互。

```loom
// 假设 raw_alloc 返回 &u8
let ptr = allocator.alloc_raw(100);

// 显式检查地址是否为 0
if ptr == 0 as &u8 {
    // 内存分配失败
}

```

对于 Loom 原生代码，推荐使用标准库中的 `Option(T)` 枚举来表达“可能不存在”的概念。对于指针类型的 `Option`，编译器会进行空指针优化，使其运行时开销等同于裸指针判空。

```loom
// 安全的写法
fn find_user(id: i32) Option(User) { ... }

```

### 类型规则

1. 类型转换

使用`as`来完成类型转换。注意，Loom中的`as`更像是一种函数或者说操作，时常会伴随编译器行为。使用`as`意味着你明确知晓并要求编译器执行某种数据变换。这种变换可能是有损的，也可能是涉及运行时计算的。比如,

* 数值截断：发生位操作。

```loom
let a: i32 = 256;
let b = a as u8; // 结果为 0。发生了位的截断操作，这是计算而非简单的“看待方式改变”。
```

* 重解释

```loom
let ptr = 0xDEADBEEF as &u8; // 强制将整数视为指针
```

无论如何，可以把`as`理解为一个动词或者说`syscall`，相当于告诉编译器，`“去做必要的工作（截断、移位、计算偏移），把这个值变成那个样子。”`

1. 类型推导

支持类型推导。

3. 泛型

在Loom中，泛型是一等公民。这里会涉及`类型表达式`的概念。
在`Zig`中，由于编译时运行和类型作为值，可以用很方便轻松的语法来表达泛型。Loom的泛型设计继承自`Zig`，但是做了一些区分和限制，从而得到了类型表达式。比如，

```loom
struct Point(X: Any, Y: Any) {
    x: X,
    y: Y
}

let p = Point(i32, i32){ x: 10, y: 20};
```

这里的`Point(i32, i32)`就是一个类型表达式。

其实可以这样理解，
```
typedef Point(X: AnyTrait, Y: AnyTrait) = struct {
    x: X,
    y: Y,
}
```
只不过这样写太复杂了，不方便也不优雅，所以按照上面的设计和语法来。

定义泛型函数则是这样,
```loom
fn add(T: Addable, a: T, b: T) T {
    a + b
}

fn main() {
    let r1 = add(i32, 10, 20);
    let r2 = add(f32, 10, 20);
}

```

`Addable`定义了T这个类型是什么类型（也就是满足什么约定）。就好比你必须要在定义函数的时候告知编译器要传入的参数是什么类型一样，你也得告诉编译器传入的类型参数是什么类型。

当然了，就好比类型有预先定义好的一样，编译器也会有一些基础的`trait`。比如描述基本运算的，描述迭代器的，当然早期会有`Any`这种万能泛型，在实际编写代码中最好不用。相对应的，使用`Any`的地方你几乎不能对这个类型做任何的操作。

## 表达式

### 运算符

* 算术运算符 `+`, `-`, `*`, `/`, `%`。
* 比较运算符 `==`, `!=`, `>=`, `<=`
* 逻辑运算符 `and`, `or`, `!`。
* 位运算符 `&`, `|`, `^`, `<<` `>>`, `~`。
* 赋值运算符 `=`以及各种来自算术运算符和位运算符中的符号与`=`排列组合。

### 优先级与结合性
表格：

### 特殊表达式

1. 函数调用: 直接调用即可。当然，由于默认值的存在（编译器会帮你写，如果不写的话），有一定的规则。

```
fn add3(a: i32, b: i32 = 0, c: i32) i32 {
    a + b + c
}

fn main() {
    add3(2, 3, 4); // 普通调用
    add3(a: 2, b: 3, c: 4) // 指定参数，可以乱序如果指定了参数，编译器会帮你重新排一下。
    add3(a: 2, c: 4) // b使用了默认参数,编译器会帮你放进去
    // 其他情况都不行。
}
```

2. 成员访问。如果是指针且指向的是结构体或者类（class，struct或者union），则可以自动解引用。

3. 索引访问。对数组，切片或者元组使用。不能对指针使用。

4. 匿名函数。 （暂时不支持）

## 语句

### 变量声明

- 可变： `var`
- 不可变： `let`
- 编译时常量： `const`

作用于规则：有遮蔽，其他类C系语言规则。

### 控制流

#### 1. 条件判断
```loom
var buffer = [0; 1024];
let stdin = std.fs.stdin();
let num = stdin.input(buffer);
if num >= 10 {
    std.debug.println("Bigger than  10");
} else {
    std.debug.println("Smaller than  10");
}
```
当然还有match
```loom
enum Result(T, E) {
    Ok(T),
    Error(E),
}

impl Result(T: Any, E: Any) {
    fn is_ok(&self) bool {
        match r {
            .Ok(_) => true,
            .Error(_) => false
        }
    }
}

fn main() {
    let r = Result(i32, []u8).Ok(10);
    if r.is_ok() {
        std.debug.println("OK!");
    } else {
        std.debug.println("Error!");
    }
}
```

`if-else`和`match`都是表达式。

#### 2. 循环

在 Loom 中，`for` 是唯一的循环关键字。它承担了 C 语言中 `for` 和 `while` 的所有职责。

为了保持语言的简单性和正交性，**`for` 结构是语句（Statement），而不是表达式**。这意味着循环不能返回值，`break` 只能用于跳转，不能携带数据。

Loom 坚持语法的**结构一致性**，因此 `for` 循环始终保持三段式结构：`for init; condition; post`。

##### 2.1 标准三段式

最经典的循环形式，包含初始化、条件检查和步进操作。

```loom
let hello: []u8 = "hello!";

// for var i = 0; i < #hello; i += 1
for var i = 0; i < #hello; i += 1 {
    std.debug.println("{}", hello[i]);
}

```

##### 2.2 条件循环 (While 语义)

当不需要初始化和步进时，`for` 依然要求保留分号以维持语法结构的完整性。这种严格的设计消除了语法歧义，让代码的视觉结构始终保持一致。

```loom
var i = 0;

// 严格保留分号，等同于 while (i < 100)
// 这种写法明确展示了 init 和 post 位置是空的
for ; i < 100; {
    std.debug.println("Counting: {}", i);
    i += 1;
}

```

##### 2.3 无限循环

当所有部分都省略时，构成无限循环。这是编写服务进程或事件循环的标准方式。
为了书写简洁，Loom 允许在无限循环时省略分号（这是 `for` 语法中唯一的特例，因为 `for {}` 极其常见且无歧义）。

```loom
// 推荐写法
for {
    if should_stop() {
        break; // 仅跳出，不返回值
    }
    do_work();
}

// 也可以写成严格模式（虽然没必要）
for ;; { ... }

```

##### 2.4 循环控制与数据获取

由于 `for` 是语句，如果你需要从循环中获取结果，必须在循环外显式声明变量（这也符合手动管理内存的哲学）。

* `break`: 跳出当前循环。不能携带返回值。
* `continue`: 跳过本次迭代剩余代码，进入 `post` 步进阶段。
* `label`: 支持带标签的跳转，用于处理嵌套循环。

```loom
// 示例：获取循环结果的标准范式
var result: i32 = undef; // 显式声明目标存储
var found = false;

'search: for var i = 0; i < 100; i += 1 {
    for var j = 0; j < 100; j += 1 {
        if matrix[i][j] == target {
            result = matrix[i][j];
            found = true;
            break 'search; // 带标签跳出多层循环
        }
    }
}

if found {
    std.debug.println("Found: {}", result);
}

```

#### 3. 跳转
`break`, `continue`, `return`

### 错误处理

推荐使用标准库`std.result`中定义的Result(T, E) 和Option(T)。标准库很多都会返回这两个。

## 函数与模块化

### 1. 函数定义

* 参数是传值的。
* 默认参数：
```loom
fn add(a: i32 = 0, b: i32 = 0) i32 {
    a + b
}
```
规则在前面的函数调用中已经说明的比较清楚了。
* 变长参数
只有与C交互时能编写变长参数。标准和C一样。其他情况请使用宏来实现类`print`机制。
* 多返回值
没有这个概念。可以用`union`或者`enum`或者元组来实现。

### 2. 方法 

#### 2.1 基本概念

方法是定义在特定类型命名空间下的函数。使用 `impl` 块将函数关联到具体的类型上。

Loom 不存在隐式的所有权转移或借用检查。通过 `self` 的不同形式，明确区分了参数传递的方式：

* `self`: **按值传递 (Pass by Value)**。
* 发生按位复制 (Bitwise Copy)。
* 对于像 `i32` 或 `Point` 这样的小型数据结构非常高效。
* **注意**：如果结构体管理着资源（如文件句柄），请谨慎使用此模式，以免造成资源重复释放或状态不一致。


* `&self`: **按指针传递 (Pass by Pointer)**。
* 语义上表示“只读访问”。底层传递的是地址。


* `&mut self`: **按可变指针传递 (Pass by Mutable Pointer)**。
* 允许方法修改实例内部数据。


* **无接收者**: 静态方法。通常用于构造函数。

```loom
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    // [静态方法]
    pub fn new(x: i32, y: i32) Point {
        Point { x: x, y: y }
    }

    // [按指针传递] - 避免复制，读取数据
    pub fn len(&self) i32 {
        self.x * self.y // 自动解引用访问
    }

    // [按可变指针传递] - 修改内存
    pub fn move_to(&mut self, new_x: i32, new_y: i32) {
        self.x = new_x;
        self.y = new_y;
    }

    // [按值传递] - 发生复制
    // 这里 p 是调用者的副本。函数结束时，副本销毁。
    pub fn copy_and_print(self) {
        println("Copy of Point ({}, {})", self.x, self.y);
    }
}

fn main() {
    var p = Point.new(10, 20);
    
    p.move_to(5, 5);      // 传递 &p
    let area = p.len();   // 传递 &p
    
    p.copy_and_print();   // 传递 p 的副本
    
    // p 依然存在且有效，因为 Loom 没有所有权转移机制
    std.debug.println("Original p is still here: {}", p.x); 
}

```

注：关于资源管理 Loom 不依赖析构函数（Destructor）自动释放资源。对于持有资源的结构体（如文件、Socket），推荐定义 init 和 deinit 方法，并配合 defer 关键字使用：
```loom
fn process_file() {
    var f = File.open("data.txt");
    defer f.close() // 作用域结束时执行，无论是否发生 return
    f.write("hello");
}
```

#### 2.2 内置特征与运算符重载 

Loom 将运算符视为特定泛型 Trait 的语法糖。要支持运算符重载，你需要实现对应的 Trait。

这些 Trait 通常带有泛型参数，以支持不同类型之间的运算（如 `Vector * float`）。

**标准库定义示例 (`std.ops`)：**

```loom
// 定义一个接受泛型参数 RHS (右操作数) 的 Trait
trait Add(RHS: Any) {
    type Output; // 关联类型：定义运算结果的类型
    fn add(self, rhs: RHS) Output;
}

```

**用户实现示例：**

```loom
use std.ops.Add;

struct Vec2 { x: f32, y: f32 }

// 实现 Vec2 + Vec2
impl Vec2: Add(Vec2) {
    type Output = Vec2; 

    fn add(self, other: Vec2) Vec2 {
        Vec2 {
            x: self.x + other.x,
            y: self.y + other.y
        }
    }
}

// 实现 Vec2 + f32 (标量加法)
// 展示了泛型 Trait 的多态性
impl Vec2: Add(f32) {
    type Output = Vec2;

    fn add(self, scalar: f32) Vec2 {
        Vec2 {
            x: self.x + scalar,
            y: self.y + scalar
        }
    }
}

fn main() {
    let v1 = Vec2{x: 1.0, y: 2.0};
    let v2 = Vec2{x: 3.0, y: 4.0};
    
    let v3 = v1 + v2;  // 调用 Vec2.add(Vec2) -> Vec2
    let v4 = v1 + 10.0; // 调用 Vec2.add(f32) -> Vec2
}

```

#### 2.3 类型表达式与实现 

在 Loom 中，类型是一等公民。`impl` 关键字不仅仅是为一个“名字”添加方法，而是为一个 **类型表达式 (Type Expression)** 的计算结果添加方法。

这意味着你不需要为了给一个数组添加辅助函数而专门包裹一个 `struct`（NewType 模式）。你可以直接对任何合法的类型布局定义行为。

##### 2.3.1 什么是类型表达式？

类型表达式是任何在编译期能求值为一个类型的表达式。
包括但不限于：

* 具体结构体名：`Point`
* 泛型实例化：`Point(i32, i32)`
* 基本类型：`i32`, `u8`
* 复合类型：`[]u8` (切片), `[4]f32` (数组), `&File` (指针)
* 函数类型：`fn(i32) void`

##### 2.3.2 更加自由的 impl

Loom 允许通过模式匹配的方式来书写 `impl`，这使得泛型实现和具体特化变得在语法上高度统一。

**示例 1：为具体类型表达式实现**

```loom
// 直接为 [4]f32 数组实现方法
// C++ 需要写全局函数，Rust 需要包裹 struct，Loom 直接 impl
impl [4]f32 {
    fn length_sq(&self) f32 {
        var sum: f32 = 0.0;
        for var i = 0; i < 4; i += 1 {
            sum += self[i] * self[i];
        }
        return sum;
    }
}

fn main() {
    let vec = [1.0, 2.0, 3.0, 4.0];
    // 就像调用对象方法一样自然，且没有运行时开销
    let len = vec.length_sq(); 
}

```

**示例 2：泛型模式匹配 (Pattern Matching)**

Rust 的 `impl<T> Point<T>` 语法将泛型参数声明和类型使用分开了。
Loom 推荐直接在类型表达式中声明泛型参数，这种 **“结构化解构”** 的写法更符合直觉。

```loom
// 语法解读：
// impl 后面紧跟的是我们要附加行为的目标类型。
// Point(T: Any) 是一个模式，它匹配任何 Point 实例，并将内部类型绑定为 T。
impl Point(T: Any) {
    fn x_val(&self) T {
        self.x
    }
}

// 特化 (Specialization)
// 当类型表达式完全匹配 Point(f32) 时，使用此实现。
// Loom 编译器会自动选择“具体度”更高的实现。
impl Point(f32) {
    fn distance(&self) f32 {
        std.math.sqrt(self.x * self.x + self.y * self.y)
    }
}

```

**示例 3：为函数类型实现 (高阶玩法)**

你甚至可以为特定签名的函数指针添加方法，从而实现类似“中间件”或“回调链”的效果。

```loom
// 定义一个处理器的函数类型别名
type Handler = fn(Request) Response;

impl Handler {
    fn run_safe(&self, req: Request) Response {
        // self 其实就是那个函数指针
        // 这里可以做一些 pre-check
        println("Before call...");
        let res = self(req); // 调用函数指针
        println("After call...");
        res
    }
}

```

### 3. 模块化

Loom 的模块系统与文件系统高度耦合。这种设计旨在减少认知负担：**你看到的文件目录结构，就是你的代码模块结构。**

#### 3.1 模块定义 (Module Definition)

在 Loom 中，模块（Module）不仅是代码的容器，也是命名空间的边界。

* **文件即模块**：每一个 `.loom` 源文件天然构成一个模块。文件名即为模块名。
* **目录即包**：一个包含源文件的文件夹可以被视为一个模块包。
* **`init.loom`**：
* 如果一个文件夹中包含 `init.loom` 文件，该文件夹被视为一个**组合模块**。
* `init.loom` 是该文件夹的入口点。当其他代码导入该文件夹名时，实际上是导入了 `init.loom` 中定义或重导出的内容。
* 这允许开发者将复杂的逻辑拆分到文件夹内的多个文件中，但对外只暴露一个干净的 `init.loom` 接口。



**示例结构：**

```text
project/
├── main.loom        (root 模块)
├── utils.loom       (模块: utils)
└── graphics/        (模块: graphics)
    ├── init.loom    (graphics 的入口)
    ├── window.loom  (子模块: graphics.window)
    └── gl.loom      (子模块: graphics.gl)

```

#### 3.2 导入路径 (Import Paths)

使用 `use` 关键字来导入模块或模块中的符号。路径解析分为**绝对路径**和**相对路径**。

**1. 绝对路径 (Absolute Paths)**

不以 `.` 开头的路径被视为绝对路径。解析顺序如下：

1. **标准库**：如 `std`。
2. **第三方包**：在构建配置中定义的依赖。
3. **项目根目录**：如果当前项目根目录下有 `math.loom`，则 `use math` 会直接定位到该文件。

```loom
use std.debug.println; // 导入标准库具体函数
use math.PI;           // 导入根目录下 math 模块的 PI 常量

```

**2. 相对路径 (Relative Paths)**

以 `.` 或 `..` 开头的路径。这在项目内部相互引用时非常有用。

* `use .submod`: 导入当前目录下的 `submod.loom` 或 `submod/init.loom`。
* `use ..parent`: 导入上一级目录的模块。

```loom
// 在 graphics/init.loom 中
use .window; // 导入同级目录下的 window.loom
use ..utils; // 导入上级目录的 utils.loom

```

#### 3.3 导入语法 (Import Syntax)

Loom 提供了灵活的导入语法来控制命名空间。

* **直接导入**：引入模块名。
```loom
use std.math;
// 调用: math.sin(1.0)

```


* **具体导入**：直接引入模块内的符号。
```loom
use std.math.sin;
// 调用: sin(1.0)

```


* **组合导入 (Grouping)**：使用 `{}` 一次性导入多个符号。
```loom
use std.math.{sin, cos, PI};

```


* **通配符导入 (Glob Import)**：使用 `*` 导入模块下所有 `pub` 可见的符号。
* *注：一般不推荐在库代码中使用，以免污染命名空间，但在脚本或测试中很有用。*


```loom
use std.math.*;

```


* **重命名 (Aliasing)**：使用 `as` 解决命名冲突或简化名称。
```loom
use std.network.tcp.Client as TcpClient;
use .window as win;

```



#### 3.4 可见性 (Visibility)

模块化必然伴随着可见性控制。Loom 遵循**默认私有**原则。

* **`pub`**: 使用 `pub` 修饰的函数、结构体、常量或 `use` 语句，可以被外部模块访问。
* **私有 (默认)**: 未加修饰的元素仅在当前文件（模块）内可见。

**init.loom 模式示例：**

假设 `graphics/window.loom` 定义了 `Window` 结构体：

```loom
// graphics/window.loom
pub struct Window { ... } // pub 使其对 graphics 包内部可见
struct InternalState { ... } // 私有，仅 window.loom 可见

```

在 `graphics/init.loom` 中重新导出，以便外部使用 `graphics.Window`：

```loom
// graphics/init.loom
// 导入子模块，并 pub 导出，这叫做 "Re-export"
pub use .window.Window; 

// 现在用户写 use graphics.Window 即可，
// 而不需要写 use graphics.window.Window

```

## 内存管理

Loom 坚持 **“手动管理，显式清理”** 的原则。语言本身不绑定任何特定的内存模型（没有 GC，没有隐式的 RAII 析构调用）。相反，Loom 提供了一套简洁的语法原语 `defer`，配合标准库的分配器接口，让开发者能够精确、安全地控制内存生命周期。

### 1. 核心哲学

* **无隐式分配**：函数调用不会在背后偷偷分配堆内存。所有的堆分配行为都必须显式调用分配器。
* **无隐式释放**：没有垃圾回收器（GC），也没有自动运行的析构函数。资源的释放必须在代码中可见。
* **机制与策略分离**：语言提供清理机制（`defer`），标准库提供分配策略（`Allocator`）。

### 2 延迟执行 

`defer` 语句用于注册一个在当前作用域（Scope）退出时执行的操作。无论作用域是因为正常执行结束、`return` 返回、还是 `break`/`continue` 跳出而结束，`defer` 注册的语句都会被执行。

* **执行顺序**：后进先出（LIFO）。最后注册的 `defer` 最先执行。
* **作用域绑定**：`defer` 绑定到最近的花括号 `{}` 块级作用域。

这使得资源管理逻辑可以紧挨着资源获取逻辑编写，极大地降低了忘记释放资源（Memory Leak）的风险。

```loom
fn process_data() {
    // 1. 获取资源
    // 假设 std.heap.page_allocator 是一个全局分配器
    var ptr = std.heap.page_allocator.alloc(u8, 1024);
    
    // 2. 注册清理 (紧随其后)
    // 当 process_data 返回时，这行代码会自动执行
    defer std.heap.page_allocator.free(ptr);

    // 3. 业务逻辑
    var ptr = match ptr {
        .None => return, // 这里也会触发 free
        .Some(p) => p,
    }
    
    // ... 对 ptr 进行操作
} // 作用域结束，执行 free
```

### 3. 分配器接口 

Loom 不内置 `malloc` 或 `new` 关键字。内存分配被抽象为标准库中的 `Allocator` 特征（Trait）。这意味着内存分配器只是一个普通的、实现了特定接口的对象。

开发者可以根据需求选择不同的分配策略，甚至编写自己的分配器（例如内核开发中的 Bump Allocator）。

```loom
// std.mem.Allocator 的概念定义
trait Allocator {
    fn alloc(self, size: usize) &u8;
    fn free(self, ptr: &u8);
}

```

**依赖注入模式**

Loom 鼓励将分配器作为参数传递给需要分配内存的函数或结构体。这种显式的“依赖注入”使得函数的内存开销一目了然。

```loom
struct List(T: Any) {
    data: []T,
    allocator: &Allocator, // 持有分配器的引用
}

impl List(T: Any) {
    // 初始化时传入分配器
    fn init(alloc: &Allocator) List(T) {
        List(T) {
            data: [], // 空切片
            allocator: alloc
        }
    }

    fn append(&mut self, item: T) {
        // 使用存储的分配器进行扩容
        let new_ptr = self.allocator.alloc(...);
        // ...
    }
    
    fn deinit(&self) {
        // 手动释放内存
        self.allocator.free(self.data.ptr);
    }
}

```

### 4. 常用分配策略

虽然语言不强制，但标准库 `std.heap` 提供了几种通用的分配器实现，覆盖了 99% 的场景：

1. **`GeneralPurposeAllocator` (GPA)**: 通用堆分配器，类似于 `malloc/free`，但在 Debug 模式下通常包含内存泄漏检测功能。
2. **`ArenaAllocator`**: 区域分配器。允许你分配大量对象，最后一次性释放整个区域。这对于处理请求生命周期或构建树状结构（如 AST）非常高效，且能避免内存碎片。
3. **`PageAllocator`**: 直接向操作系统申请整页内存。

```loom
fn main() {
    // 创建一个 Arena
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    // 确保整个 Arena 在 main 结束时释放
    defer arena.deinit();

    // 从 Arena 分配，不需要单独释放 list
    // 哪怕 list 内部申请了很多次内存，arena.deinit() 会一波带走
    var list = List(i32).init(arena.allocator());
    list.append(1);
    list.append(2);
}

```

## 宏系统 

Loom 提供了一个强大且卫生的声明式宏系统。虽然其模式匹配语法借鉴自 Rust，但在使用体验上进行了大幅简化：没有 `macro_rules!` 这种冗长的关键字，调用时也不需要 `!` 后缀。

宏在 Loom 中被视为一种 **AST 变换机制**，而非简单的文本替换（C 风格）。这意味着宏是卫生的（Hygienic），不会意外捕获外部变量，且必须生成合法的语法树。

### 1 定义与调用

使用 `macro` 关键字定义宏。宏定义支持重载（通过模式匹配区分），并且与其所在模块的命名空间深度绑定。

**特性：**

* **统一调用语法**：宏的调用与函数调用在语法上完全一致。
* **乱序定义**：宏可以在文件的任何位置定义，上方的代码可以调用下方定义的宏。
* **模块化**：宏遵循标准的模块可见性规则 (`pub`)。

```loom
// 定义一个简单的断言宏
// 即使定义在 main 下方，main 依然可以调用它
pub macro assert {
    // 模式：匹配一个表达式 ($cond)
    ($cond:expr) => {
        if !$cond {
            std.debug.panic("Assertion failed!");
        }
    };
    
    // 模式重载：支持自定义消息
    ($cond:expr, $msg:expr) => {
        if !$cond {
            std.debug.panic($msg);
        }
    };
}

fn main() {
    let x = 10;
    // 调用宏，就像调用函数一样，不需要 assert!(...)
    assert(x > 5); 
    assert(x < 100, "X is too big");
}

```

### 2 模式匹配与指示符

Loom 的宏通过匹配参数的语法结构来工作。参数以 `$` 开头，后跟类型指示符。

常用的指示符（Fragment Specifiers）：

* `expr`: 表达式 (例如 `1 + 2`, `func()`)
* `ident`: 标识符 (例如变量名、类型名 `x`, `Point`)
* `ty`: 类型 (例如 `i32`, `&User`)
* `literal`: 字面量 (例如 `"hello"`, `42`)
* `block`: 代码块 (例如 `{ ... }`)
* `stmt`: 语句 (例如 `let x = 1;`)

**重复模式**

Loom 支持对参数进行重复匹配，通常用于处理变长参数列表。语法为 `$(...)*` 或 `$(...),*`（逗号分隔）。

```loom
// 模拟 vec 创建
macro vec_of {
    // 匹配：($elem:expr),* // 表示匹配零个或多个由逗号分隔的表达式
    ( $( $elem:expr ),* ) => {
        {
            var temp_arr = std.list.new();
            $(
                // 这一行代码会根据参数数量被重复展开
                temp_arr.append($elem);
            )*
            temp_arr
        }
    };
}

fn main() {
    // 展开为一系列 append 调用
    let v = vec_of(1, 2, 3, 4);
}

```

### 3 卫生性 (Hygiene)

Loom 的宏是卫生的。这意味着宏内部定义的变量不会污染外部作用域，除非你显式传递了标识符。

```loom
macro strict_math {
    ($a:expr, $b:expr) => {
        {
            let temp = $a + $b; // temp 是宏内部的临时变量
            temp * 2
        }
    }
}

fn main() {
    let temp = 100; // 这里的 temp 不会被宏内部的 temp 覆盖
    let res = strict_math(1, 2); 
    // res = 6, temp 依然是 100
}

```

### 4 为什么没有 `!` ?

在 Rust 中，`!`用于区分宏和函数，因为宏可能无法像函数那样被类型检查，或者具有特殊的控制流行为（如 `return`）。

在 Loom 中，我们选择了**视觉上的极简主义**。

* **一致性**：对于使用者来说，`println("hello")` 就是一个执行打印的操作，用户并不关心底层是函数调用还是代码展开。
* **编译期处理**：Loom 编译器在解析阶段就能通过符号表分辨出目标是 `macro` 还是 `fn`。
* **显式导入**：由于必须 `use std.debug.println` 才能使用，导入路径明确告知了这是一个来自标准库的功能，消除了混淆的风险。

## 外部函数接口 

Loom 将与 C 语言的互操作性视为头等大事。FFI 的设计目标是零开销且直观。Loom 默认仅支持 C 语言的 ABI（应用程序二进制接口）。

### 1. 引入外部符号

使用 `extern` 块声明外部 C 函数或全局变量。在 `extern` 块中声明的符号默认遵循 C 调用约定（C Calling Convention）。

```loom
// 声明外部 C 函数
extern {
    fn printf(fmt: &u8, ...) i32;
    fn malloc(size: usize) &mut u8;
    fn free(ptr: &mut u8);
}

fn main() {
    let msg = "Hello from C!\n\0";
    printf(&msg[0]); 
}

```

### 2. 结构体布局

这是 Loom 与 C 交互时最重要的区别：

* **普通结构体 (`struct`)**：Loom 编译器**不保证**字段在内存中的顺序。为了优化内存占用（减少 Padding），编译器会自动重排字段顺序。因此，普通的 Loom 结构体不能直接传给 C 代码。
* **外部结构体 (`extern struct`)**：使用 `extern` 修饰的结构体将强制采用 **C 兼容布局**。字段将严格按照声明顺序排列，并遵循标准 C 的内存对齐规则。

```loom
// [Loom 布局]
// 编译器可能会重排字段（例如把 x 和 z 放在一起）以节省空间
struct OptimizeMe {
    x: u8,
    y: i64, // 8字节
    z: u8,
}

// [C 布局]
// 严格匹配 C 语言：u8 -> padding(7) -> i64 -> u8 -> padding(7)
// 可以安全地传给 C 函数
extern struct CRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

```

### 3. 类型兼容性

除结构体布局外，Loom 的基本类型设计旨在与 C 保持高度兼容：

* **整数/浮点数**：`i32`, `u8`, `f64` 等类型与 C 的对应类型二进制兼容。
* **指针**：Loom 的指针 `&T` 和 `*T` 在 ABI 层面等同于 C 指针。
* **函数**：被标记为 `extern` 的 Loom 函数也会使用 C 调用约定，从而可以被 C 代码回调。

```loom
// 导出给 C 调用的函数
pub extern fn loom_callback(val: i32) {
    std.debug.println("Called from C with: {}", val);
}

```

