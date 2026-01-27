# 基础类型与字面量 (Basic Types & Literals)

Loom 提供了一组固定宽度的基础类型，以确保跨平台的行为一致性。

## 1. 原始类型 (Primitives)

| 类型                      | 描述         | 位宽 (Bit Width)    |
| ------------------------- | ------------ | ------------------- |
| `i8`, `i16`, `i32`, `i64` | 有符号整数   | 8, 16, 32, 64       |
| `u8`, `u16`, `u32`, `u64` | 无符号整数   | 8, 16, 32, 64       |
| `usize`, `isize`          | 平台相关整数 | 指针宽度 (32/64)    |
| `f32`, `f64`              | 浮点数       | IEEE-754 标准       |
| `bool`                    | 布尔值       | 1 byte (true/false) |

> **注：** `char` 在 Loom 中不是独立类型，字符字面量 `'c'` 等同于 `u8` (ASCII)。

## 2. 数组 (Arrays)

数组是**固定长度**的、分配在**栈 (Stack)** 上的连续内存块。

* **类型语法：** `[N]T` (例如 `[10]i32`)
* **字面量：** `[1, 2, 3]`
* **长度推导：** 使用 `_` 让编译器根据初始值计数。

```loom
let a: [5]i32 = [1, 2, 3, 4, 5];
let b: [_]u8 = [10, 20]; // 推导为 [2]u8

```

### 2.1 获取长度

使用前缀操作符 `#` 获取数组长度（编译期常数）。

```loom
let len = #a; // 5

```

### 2.2 转指针

数组名本身**不会**隐式退化为指针。必须显式取首元素地址。

```loom
let ptr: *i32 = &a[0];

```

## 3. 切片 (Slices)

切片是对连续内存的动态视图。它是**胖指针 (Fat Pointer)**，由 `(ptr, len)` 两部分组成。

* **类型语法：** `[]T` (不可变), `[]mut T` (可变)
* **获取长度：** 同样使用 `#` 操作符（运行时读取胖指针中的 len 字段）。

### 3.1 创建切片

1. **从数组隐式转换：** 传递数组给接受切片的函数时自动转换。
2. **指针构造 (Unsafe)：** 使用范围语法 `ptr[start..end]`。需要程序员确保内存安全。

```loom
let arr: [10]i32 = undef;
let ptr: *i32 = &arr[0];

// 指针 切片 (Range Syntax)
// start..end (左闭右开)
let slice: []i32 = ptr[0..4]; 

// 获取长度
let l = #slice; // 4

```

## 4. 字符串 (Strings)

在 Loom 中，**字符串不是一种独立的类型**，它纯粹是 **`u8` 数组** 的语法糖。Loom 坚持“字节即真理”的原则，不内置复杂的编码猜测或隐式转换。

### 4.1 语法糖本质

字符串字面量 `"hello"` 在编译期会被直接展开为等价的字符数组字面量 `['h', 'e', 'l', 'l', 'o']`。

这意味着字符串的行为完全遵循 **数组 (Array)** 和 **切片 (Slice)** 的通用规则：

* **存储位置：** 取决于绑定关键字（`let` 在栈上，`static` 在静态区）。
* **类型：** 取决于显式标注或推导，可以是定长数组 `[N]u8`，也可以是切片 `[]u8`。
* **编码：** 统一为 **UTF-8** 编码。这意味着 `'a'` 占用 1 字节，`'中'` 可能占用 3 字节。

### 4.2 定义与存储

由于字符串只是数组字面量，开发者拥有完全的内存控制权。

```loom
// 1. 栈上数组 (Stack Array)
// 等价于: let a: [5]u8 = ['h', 'e', 'l', 'l', 'o'];
// 数据直接填充在栈内存中，适合短字符串或临时拼接
let a: [5]u8 = "hello"; 

// 2. 栈上切片 (Stack Slice)
// 编译器在栈上创建一个临时匿名数组，s 是指向它的切片 (ptr, len)
let s: []u8 = "world";

// 3. 静态数组 (Static Array)
// 等价于: static data: [5]u8 = ...
// 数据存储在 .data/.rodata 段，程序生命周期内有效
static data: [_]u8 = "hello"; 

// 4. 静态切片 (Static Slice)
// 只有在 static 定义中，切片才指向静态区的内存
static slice: []u8 = "world";

```

> **最佳实践：** 对于不需要修改的长字符串常量，建议使用 `static` 以减少栈内存开销和运行时的拷贝开销。

### 4.3 字符字面量

Loom 仅支持单字节字符字面量。

* `'a'` : 类型固定为 `u8`。
* **不支持** `'中'` 这种多字节字符作为单字符字面量（因为它实际上是多个 `u8`）。若需处理 Unicode 字符，请直接使用字符串字面量 `"中"`。

```loom
let c: u8 = 'a';
// let z: u8 = '中'; // 编译错误：Character literal is too wide
let z_str: []u8 = "中"; // 正确：这是包含 3 个 u8 的数组

```

### 4.4 常用操作

由于字符串即 `[]u8`，所有切片和数组的操作均适用：

```loom
let str = "hello";
let len = #str;       // 5
let h = str[0];       // 'h'
let sub = str[1..3];  // "el" (类型为 []u8)

```

## 5. 元组 (Tuples)

用于组合不同类型的值。

* **类型语法：** `(T1, T2, ...)`
* **字面量：** `(1, true, "ok")`
* **访问：** 使用 `tuple[0]`, `tuple[1]`。

### 5.1 单元类型 (Unit)

空元组 `()` 是 Loom 中的特殊类型，表示“无值”。

* **不存在 Void：** 函数没有返回值时，实际返回的是 `()`。
* **禁止标记：** 禁止显式写 `fn ()`，直接省略返回类型即可。

---



# 常量与编译时求值 (Constants & CTFE)

Loom 区分 **运行时 (Runtime)** 与 **编译时 (Compile-time)**。`const` 关键字用于定义在编译阶段必须确定的值或逻辑。

## 1. 常量定义 (`const`)

常量是具名的、不可变的、且在编译期已知的值。

* **语法：** `const NAME: Type = Expression;`
* **语义：**
* 编译器在编译阶段计算表达式的值。
* 在生成的二进制代码中，常量通常被内联（Inlined）到使用处，或者作为只读数据段（`.rodata`）的一部分。
* **替代宏：** 此时应优先使用 `const` 而非 `macro`，因为 `const` 有类型检查且遵循作用域规则。



```loom
// 推荐：有类型检查，命名空间清晰
const MAX_BUFFER: usize = 1024;
const PI: f64 = 3.1415926;

// 可以在局部作用域定义
fn process() {
    const LOCAL_LIMIT: i32 = 10;
    // ...
}

```

## 2. 编译时函数 (`const fn`)

`const fn` 是一类特殊的函数，它们既可以在编译期执行，也可以在运行期执行。

* **语法：** `const fn name(...) ret { ... }`
* **约束：**
* 函数体内的逻辑必须是 **确定性的 (Deterministic)**。
* **禁止：** I/O 操作、访问全局可变变量、外部函数调用（除非该 extern 也被标记为 const 且编译器支持）、非确定性内存分配。
* **允许：** 算术运算、逻辑控制（if/match/loop）、局部变量。



```loom
const fn align_up(val: usize, align: usize) usize {
    let remainder = val % align;
    if remainder == 0 {
        return val;
    }
    return val + (align - remainder);
}

// 编译时调用：计算结果直接作为常量 1024
const BUFFER_SIZE: usize = align_up(1000, 8); 

// 运行时调用：像普通函数一样执行指令
let x = align_up(runtime_val, 8); 

```

## 3. 编译时上下文 (Const Contexts)

在以下位置，Loom 强制要求表达式必须能在编译期求值：

1. **数组长度：** `let arr: [10]i32` 中的 `10`。
2. **常量初始化器：** `const X = ...` 的右值。
3. **枚举判别值：** `enum E { A = 1, B = A + 1 }`。
4. **模式匹配的范围：** `match x { 0..MAX => ... }`。



# 类型转换 (Type Conversion)

Loom 坚持 **严格类型 (Strict Typing)** 原则。不存在隐式的类型提升（Promotion）或强制转换（Coercion）。所有的类型转换必须通过 `as` 关键字显式进行。

为了平衡严格性与开发体验，Loom 提供了 **Binding Cast** 语法糖，用于简化函数调用时的参数转换。

## 1. 显式转换 (`as` Operator)

`as` 是一个二元操作符，用于执行大多数“常规”的类型转换。

* **语法：** `expression as TargetType`
* **语义：** 尝试将值转换为目标类型。如果转换在语义上是不可能的（例如将 `struct` 转为 `f32`），则报编译错误。

### 1.1 数值转换

数值类型之间的转换遵循底层 CPU 指令行为：

* **扩宽 (Widening):** `u8 as u32`。零扩展（Unsigned）或符号扩展（Signed）。总是安全的。
* **截断 (Truncating):** `u32 as u8`。保留低位，丢弃高位。**注意：** 这是一个有损操作，且不会发生运行时 Panic。
* **浮点转换:** `f32 as i32`。向零取整（Truncate decimal part）。

### 1.2 指针与引用转换

* **切片继承 (Upcasting):** `&Child as &Parent`。零开销，仅仅改变类型标签。
* **去借用 (Escape):** `&T as *T`。将安全的引用转换为裸指针，放弃借用检查。
* **地址获取:** `*T as usize`。获取指针的内存地址。

```loom
let big: i32 = 1024;
let small: u8 = big as u8; // 结果为 0 (1024 % 256)

let ptr: *i32 = &big as *i32;
let addr: usize = ptr as usize;

```

## 2. 绑定转换 (Binding Cast)

为了减少 API 调用时的样板代码，Loom 允许在函数参数声明中使用 `as` 修饰符。

* **语法：** `arg_name: as Type`
* **原理：** 这是一个**纯粹的语法糖**。编译器在调用点（Call Site）自动插入 `as Type` 操作。

### 2.1 示例

假设我们有一个底层绘图 API，需要接受不同宽度的整数：

```loom
// 定义：参数使用 binding cast
fn set_color(r: as u8, g: as u8, b: as u8, alpha: as f32) {
    // 在函数体内部，r, g, b 就是 u8 类型
    // alpha 就是 f32 类型
}

fn main() {
    let r_int: i32 = 255;
    let alpha_double: f64 = 1.0;

    // 调用：直接传入不同类型，编译器自动处理
    // 等价于 set_color(r_int as u8, 0 as u8, 0 as u8, alpha_double as f32)
    set_color(r_int, 0, 0, alpha_double);
}

```

### 2.2 优势与限制

* **优势：** 极大地简化了字面量（通常默认为 `i32` 或 `f64`）的传递。
* **限制：** 只能处理 `as` 操作符允许的转换。如果传入的类型无法 `as` 到目标类型（例如传入结构体），依然会报编译错误。

## 3. 内置转换函数 (`@` Intrinsics)

对于 `as` 无法处理的特殊转换（通常涉及到底层位操作或危险的内存解释），Loom 提供以 `@` 开头的编译器内置函数。

### 3.1 位转换 (@bitcast)

重新解释值的二进制表示，而不改变底层的位。要求源类型和目标类型**大小必须相等**。

* **语法：** `@bitcast.<TargetType>(Expression)`

```loom
let f: f32 = 1.0;
// 将 float 的位模式直接看作 u32
let bits: u32 = @bitcast.<u32>(f); 

```



# 变量与存储 (Bindings & Storage)

Loom 显式区分存储位置（栈、静态区）和可变性。

## 1. 变量绑定

| 关键字    | 存储位置   | 可变性 | 初始化要求 | 描述                                                   |
| --------- | ---------- | ------ | ---------- | ------------------------------------------------------ |
| `let`     | Stack      | 不可变 | 必须       | 局部变量，默认不可变                                   |
| `let mut` | Stack      | 可变   | 必须       | 局部可变变量                                           |
| `const`   | CTFE       | N/A    | 必须       | **编译期常量**，无运行时内存地址（除非取址），内联替换 |
| `static`  | .data/.bss | 不可变 | 必须       | **全局/静态变量**，生命周期伴随程序全程                |

> **注意：** `static` 变量默认是只读的（放在 .rodata），若需可变需标记 `static mut`（通常需要同步机制访问）。

## 2. 返回值处理 (Must Use)

Loom 强制要求处理函数返回值，防止忽略错误或副作用。

* 如果函数返回非 `()` 类型，调用者必须使用 `let` 绑定或处理该值。
* **显式忽略：** 使用 `_` 丢弃不需要的返回值。

```loom
fn calc()  i32 { 100 }

fn main() {
    calc();      // 编译错误：Unused return value
    let _ = calc(); // 正确：显式丢弃
}

```

# 指针 (Pointers)

本语言提供三种指针类型，分别用于不同的语义场景。Loom 的指针设计哲学是 **“显式优于隐式”** 和 **“机制轻量化”**。我们不引入复杂的生命周期或借用检查器，而是通过类型系统的约束来表达程序员的意图。

所有标准指针类型（`&T`, `*T`, `^T`）在语义上均被视为 **非空 (Non-nullable)**。编译器保证这些类型的变量在运行时持有一个有效的内存地址。可空指针请参阅 **[可空类型 (?T)]** 章节。

## 1. 类型概览

| 类型符号        | 名称         | 绑定重指向 (Rebinding) | 算术运算 | 读写权限 | 用途                       |
| --------------- | ------------ | ---------------------- | -------- | -------- | -------------------------- |
| `&T`            | **只读引用** | **不可**               | 否       | 只读     | 默认的参数传递方式         |
| `&mut T`        | **读写引用** | **不可**               | 否       | 读写     | 需要修改数据的场景         |
| `*T` / `*mut T` | **原生指针** | 允许                   | 是       | 视修饰符 | 底层内存操作，数据结构实现 |
| `^T` / `^mut T` | **易失指针** | 允许                   | 是       | 视修饰符 | IO、驱动开发、寄存器操作   |

> **核心区别：**
> * **引用 (`&`)** 是 **“锚定”** 的指针。一旦初始化，它就不能指向别处，且不支持指针算术。这让它非常适合作为函数参数。
> * **原生指针 (`*`)** 是 **“自由”** 的指针。它可以像 C 指针一样随意移动和赋值。
> 
> 

## 2. 引用 (&T)

引用是 Loom 中最常用的间接访问方式。它本质上是一个 **非空且绑定不可变的指针**。

### 2.1 绑定不可变性 (Binding Immutability)

引用变量本身是不可变的（Immutable Binding）。这意味着你不能改变它指向的地址，但可以通过 `&mut T` 改变它指向的数据。

* **规则：** 引用必须在声明时初始化，且之后不能被赋值。
* **优势：** 这让代码的数据流向非常清晰——在引用的生命周期内，它永远指向同一个对象。

```loom
let mut x = 10;
let mut y = 20;

// r1 永远指向 x
let r1: &i32 = &x; 
// r1 = &y; // ❌ 编译错误：无法对引用变量赋值

// w1 永远指向 x，但允许修改 x 的值
let w1: &mut i32 = &mut x;
w1.* = 30; // ✅ 合法：修改数据

```

### 2.2 无借用检查 (No Borrow Checking)

Loom **不强制**执行读写互斥锁或生命周期检查。这意味着编译器不会阻止你在同一作用域内同时拥有 `&T` 和 `&mut T`。

内存安全的责任主要由程序员承担。`&T` 的存在主要是为了提供非空保证和清晰的 API 语义，而非强制的内存安全保证。

```loom
let mut data = 100;

let r = &data;     // 获取只读引用
let w = &mut data; // 获取可变引用 (在 Rust 中会报错，在 Loom 中合法)

w.* = 200;         // 修改数据
print(r.*);        // 读取数据 (输出 200)

```

### 2.3 栈安全检查 (Stack Safety)

虽然没有借用检查，但 Loom 保留了一个极低成本的静态检查：**禁止返回局部栈变量的引用**。这能防止最基础的悬垂指针错误。

```loom
fn dangerous() &i32 {
    let x = 10;
    return &x; // ❌ 编译错误：Reference to local variable 'x' escapes
}

```

## 3. 原生指针 (*T) 与 易失指针 (^T)

这两种指针提供了完全的 C 语言级内存操作能力。

### 3.1 自由操作

`*T` 和 `^T` 类型的变量如果被声明为 `let mut`，则可以随时指向新的地址。它们支持指针算术运算（`+`, `-`），步长为 `sizeof(T)`。

```loom
let arr = [1, 2, 3];
// 将引用转换为原生指针以进行算术运算
let mut ptr: *i32 = &arr[0] as *i32; 

ptr = ptr + 1; // 指向 arr[1]
print(ptr.*);  // 输出 2

```

### 3.2 易失语义 (Volatile Semantics)

`^T` 是驱动开发的专用类型。编译器对通过 `^T` 进行的读写操作（`.*`）施加以下限制：

* **禁止优化：** 禁止删除、合并或重排相关的读写指令。
* **副作用保证：** 即使写入的值未被读取，写入指令也会被保留（用于触发硬件寄存器副作用）。

## 4. 操作与解引用

### 4.1 解引用 (Dereference)

所有指针类型均使用后缀 `.*` 进行解引用。

* **语法：** `ptr.*`
* **安全性：** 由于标准指针类型 (`&T`, `*T`, `^T`) 均保证非空，解引用操作不会触发空指针异常。

### 4.2 禁止下标访问

Loom **不支持**在指针上直接使用下标语法 `ptr[N]`。
若需访问偏移地址的数据，必须显式使用指针算术结合解引用：

```loom
// 假设 ptr 是 *i32
let val = (ptr + 2).*; // 正确：访问偏移量为 2 的元素
// let val = ptr[2];   // 错误：不支持此语法

```

# 可空类型与初始化 

本语言通过类型系统强制区分“有值”与“无值”。默认情况下，所有类型（如 `i32`, `&T`）均为**不可空**。若需表示值的缺失，必须显式使用可空类型修饰符 `?`。

此外，本语言严格区分 **运行时空值 (`null`)** 与 **编译期未初始化状态 (`undef`)**。

## 1. 可空类型 (?T)

`?T` 是一个包含 `T` 类型的所有值以及一个特殊值 `null` 的联合类型（Tagged Union）。

* **语法：** `let x: ?i32 = null;`
* **内存布局：**
* 对于指针类型（如 `&T`, `*T`），编译器进行**空指针优化**，`null` 表示为全 0 地址，不占用额外空间。
* 对于非指针类型（如 `i32`），`?i32` 占用 `sizeof(T) + alignment`（通常增加一个 bool 标记位）。

## 2. Null vs Undef

这两个概念在语义上完全不同，编译器对它们的处理方式处于不同阶段。

| 特性         | `null`                 | `undef`                             |
| ------------ | ---------------------- | ----------------------------------- |
| **定义**     | 表示“逻辑上的无值”     | 表示“尚未赋值的内存槽位”            |
| **存在阶段** | 运行时 (Runtime Value) | **编译期状态 (Compile-time State)** |
| **操作**     | 只能通过特定操作符解包 | **禁止任何读取操作**                |
| **用途**     | 表达可选值、缺失数据   | 延迟初始化、声明后赋值              |
| **类型**     | 属于 `?T` 的合法值     | 不属于任何类型的值                  |

### 2.1 未定义 (undef)

`undef` 不是一个值，而是一个标记。它告诉编译器：“在这里分配栈空间，但我稍后会手动填充它。”

* **安全检查：** 编译器必须进行**确定性赋值分析 (Definite Assignment Analysis)**。如果在写入 `undef` 变量之前尝试读取它，必须报编译错误。

```loom
let a: i32 = undef;
// print(a); // 编译错误：Use of uninitialized variable 'a'
a = 10;
print(a);   // 合法

```

## 3. 操作符语义 (Operator Semantics)

由于 `?T` 是一个联合类型，不能直接参与 `T` 类型的运算。Loom 提供了两个核心操作符来处理空值，分别针对 **“提供默认值”** 和 **“向上传播错误”** 这两种截然不同的场景。

### 3.1 空值合并符 (`?`) —— "Unwrap Or"

这是一个二元操作符，用于处理“如果不为空则使用值，否则执行备选逻辑”的场景。

* **语法：** `lhs ? rhs`
* **语义逻辑：**
```loom
if lhs != null {
    return lhs (as T)
} else {
    return rhs // rhs 的求值结果或控制流跳转
}

```

* **典型用途：**
1. **提供默认值：** `let x = input ? 0;`
2. **崩溃/断言：** `let ptr = get_ptr() ? @panic("Fatal");`
3. **复杂补救：** `let cfg = read() ? { log_warn(); default_cfg };`

### 3.2 传播解包符 (`.?`) —— "Propagate"

这是一个后缀一元操作符，专门用于**在函数内部快速处理空值**。它的行为是：“如果不为空则取值，如果为空则立即从当前函数返回 `null`”。

* **语法：** `expr.?`
* **语义逻辑：**
```loom
let temp = expr;
if temp == null {
    return null; // 立即终止当前函数
}
yield temp (as T); // 表达式结果为解包后的值

```


* **限制：** 只能在返回类型为可空类型（`?U`）的函数内部使用。

### 3.3 为什么我们需要 `.?` (The Necessity)

虽然 `?` 操作符配合 `{ return null }` 在理论上可以覆盖传播的需求，但在处理**链式访问**或**多步依赖**时，如果不提供 `.?` 语法糖，代码将变得极其臃肿且难以阅读。

**对比示例：访问链表深层节点**

假设我们需要获取 `root.next.next.val`，且中间任意环节都可能为空。

**方案 A：仅使用 `?` (不推荐)**
必须使用括号来控制求值顺序，且重复编写返回逻辑。

```loom
// 语法噪音极大，阅读视线被反复打断
let val = ((root ? { return null }).next ? { return null }).val;

```

**方案 B：使用 `.?` (Loom 推荐)**
逻辑呈线性流淌，清晰地表达了“尝试访问，不行就撤”的意图。

```loom
// 干净、直观、符合直觉
let val = root.?.next.?.val;

```

> **设计哲学：** Loom 坚持显式控制流，但在“错误传播”这一超高频场景下，`.?` 提供的可读性价值远远超过了其隐式返回带来的认知成本。它是系统编程中处理 `Option/Result` 链条的最佳实践。
> 
## 4. 类型收缩 (Flow Typing)

编译器具备流敏感分析能力。在某些控制流分支中，如果能确定变量不为 `null`，则该变量在该分支内自动被视为 `T` 类型。

* **显式检查：** 使用 `== null` 或 `!= null`。

```loom
let x: ?i32 = get_value();

if (x != null) {
    // 在此块内，x 的类型被收缩为 i32
    // 可以直接使用，无需解包
    let y: i32 = x + 1; 
}

```

# 控制流 (Control Flow)

本语言在设计上倾向于 **表达式 (Expression)** 优先，但保持了必要的语句 (Statement) 结构以维持底层控制的清晰度。

值得注意的是，所有的控制流关键字（`if`, `match`, `for`）均**不需要**使用圆括号包裹条件或参数。

## 1. 条件分支 (If Expression)

`if` 既是控制语句，也是表达式。

* **基本语法：** `if condition { block }`
* **无括号：** 条件表达式 `condition` 周围不加括号。
* **强制 Else 规则：**
* 当 `if` 仅作为**语句**使用时，`else` 块是可选的。
* 当 `if` 作为**表达式**（用于赋值、作为返回值、作为函数参数）使用时，**必须包含 `else` 块**，且所有分支返回值的类型必须兼容。这是为了保证表达式在任何运行时路径下都有值。



```loom
// 场景 1: 语句 (Statement)
if x > 10 {
    print("Large");
} // 合法，没有 else

// 场景 2: 表达式 (Expression)
let val = if x > 10 {
    100
} else {
    0
}; // 合法，必须有 else

// let err = if x > 10 { 1 }; // 非法！编译错误：If expression must have an else block

```

明白了，这个设计非常 **克制**。这相当于把 C 语言的 `switch` 进行了两个维度的升级：

1. **表达式化**：可以返回值。
2. **支持 ADT**：这是唯一的“模式解构”入口。

去掉了切片匹配、守卫、引用绑定等特性后，`match` 变得非常纯粹：**它就是用来做分支跳转的，而不是用来做复杂逻辑判断的。** 如果需要复杂逻辑，请用 `if`。

指针匹配被定义为地址匹配（`usize`），这非常符合 Loom 的底层定位——在系统编程中，检查指针是否等于某个特定地址（比如 `0xFFFF_FFFF` 或哨兵节点）是很常见的需求，但匹配指针指向的内容（解引用匹配）往往隐含了内存访问开销，Loom 选择不隐式去做，非常合理。

以下是重写后的 **第2节：模式匹配**。

---

## 2. 模式匹配 (Match Expression)

`match` 是 Loom 中的分支控制表达式。它的核心逻辑非常简单：**基于值的相等性跳转**。
唯有在匹配 **枚举 (Enum)** 时，它才提供结构化数据的解构能力。

* **语法：** `match target { ... }`
* **穷尽性 (Exhaustiveness)：** 编译器强制要求覆盖所有可能的情况。对于非枚举类型（如整数），必须包含 `_` (default) 分支。

### 2.1 基础值匹配

对于整数、布尔值、字符等原始类型，`match` 仅进行值的比较。

* **多重匹配：** 使用逗号 `,` 分隔多个值（例如 `200, 201`）。
* **范围匹配：** 支持 `start..=end`（闭区间）语法。

```loom
let code = 200;

let status = match code {
    200, 201 => "Success",     // 逗号分隔
    400..=499 => "Client Error", // 范围匹配
    500, 502 => "Server Error",
    _         => "Unknown"     // 必须处理默认情况
};

```

> **注：** 字符范围 `'a'..='z'` 本质上是 ASCII 码数值的范围匹配（等价于 `97..=122`）。

### 2.2 枚举解构 (Enum Destructuring)

这是 `match` 唯一具备“模式匹配”特性的场景。支持使用点号前缀 `.Variant` 进行匹配并解构数据。

```loom
enum Message {
    Move { x: i32, y: i32 },
    Color(u8, u8, u8),
    Quit
}

fn process(msg: Message) {
    match msg {
        // 结构体解构
        .Move { x, y } => print(x, y),
        
        // 元组解构 (支持使用 _ 忽略字段)
        .Color(r, _, _) => set_red(r),
        
        // 单元变体
        .Quit => exit()
    }
}

```

### 2.3 指针匹配 (Pointer Matching)

在 Loom 中，指针（`*T` 或 `&T`）没有任何特殊待遇，它们被视为**纯粹的值 (Pure Values)**。

因此，对指针进行 `match` 的行为与对 `i32` 或 `u8` 完全一致：它比较的是**变量自身的值**（即内存地址的相等性），而不是它所指向的数据。这里没有任何隐式的解引用或类型转换。

* **机制：** 基于地址的相等性检查（Identity Comparison）。
* **用途：** 检查空值、特定的硬件地址、或哨兵对象。

```loom
// 假设 get_ptr 返回一个可空指针
let ptr: ?*i32 = get_ptr();

// 定义一个哨兵地址常量 (纯粹的值)
const SENTINEL: *i32 = 0xDEAD_BEEF as *i32;

match ptr {
    // 匹配空值 (零地址)
    null => print("Is Null"),
    
    // 匹配特定值的地址
    // 就像匹配整数 100 一样自然，比较的是 ptr == SENTINEL
    SENTINEL => print("Hit Sentinel"),
    
    // 匹配任意其他地址
    _ => print("Valid Pointer")
}

```

> **注：** 如果想匹配指针指向的内容，必须显式解引用：`match ptr.* { ... }`。此时匹配的对象不再是指针，而是指针背后的数据。

## 3. 循环 (For Loop)

与其他控制流不同，**`for` 循环严格作为语句 (Statement) 存在**，不产生返回值。
采用经典的 C 语言三段式结构，但这三个部分均可独立省略。

* **语法：** `for init; condition; post { body }`
* **限制：**
* 不支持标签 (Labels)。
* 不支持 `goto` 跳转。



### 3.1 常见用法

```loom
// 标准用法
for let mut i = 0; i < 10; i += 1 {
    print(i);
}

// 类似 while 的用法 (省略 init 和 post)
let mut x = 0;
for ; x < 100; {
    x += 1;
}

// 无限循环 (全部省略)
for ;; {
    if should_stop() {
        break; // 跳出循环
    }
}

```

## 4. 延迟执行 (Defer)

`defer` 用于注册在当前作用域退出时执行的操作。

* **语法：** `defer expression;`
* **语义：**
* 执行顺序为 **后进先出 (LIFO)**。
* 通常用于资源释放（关闭文件、释放锁、释放内存）。



```loom
fn process() {
    let ptr = alloc(1024);
    defer free(ptr); // 即使下面 panic 或 return，这里也会执行

    if ptr == null {
        return; 
    }
    // ... 业务逻辑
}

```

# 数据结构 (Data Structures)

本语言的数据结构设计旨在兼顾高层抽象（ADT）与底层内存布局的确定性（继承与切片）。

## 1. 结构体 (Struct)

结构体是命名字段的集合。与部分语言不同，本语言**不支持**元组结构体（Tuple Structs, 如 `struct Color(i32, i32)`）或单元结构体（Unit Structs, 如 `struct Empty;`）。所有结构体必须包含明确命名的字段。

### 1.1 定义与布局

* **语法：** `struct Name { field: Type, ... }`
* **内存布局 (Layout)：**
* 默认情况下，编译器会对字段顺序进行**重排 (Reordering)** 以减少内存填充（Padding），优化空间占用。
* 若需与 C 语言交互或对应硬件寄存器布局，需使用 `extern struct`（详见 FFI 章节）。



### 1.2 配置式驱动 (Configuration-Driven)

Loom 支持在结构体定义时直接指定字段的默认值。这使得结构体可以用作灵活的“配置对象”，初始化时无需列出所有字段。

* **语法：** `field: Type = value`
* **初始化规则：**
* 如果字段没有默认值：必须显式赋值。
* 如果字段有默认值：可以省略，省略时自动填入默认值。
* **注意：** 不需要使用 `..` 或 `..default` 语法，编译器会自动补全未提及且有默认值的字段。

```loom
struct WindowConfig {
    width: i32 = 800,        // 有默认值
    height: i32 = 600,       // 有默认值
    title: []u8,             // 无默认值，必须提供
    fullscreen: bool = false // 有默认值
}

// 合法初始化
// 仅提供了必要的 title，其他自动使用 default
let conf = WindowConfig {
    title: "My App" 
}; 

// 显式覆盖部分默认值
let conf_hd = WindowConfig {
    title: "HD App",
    width: 1920,
    height: 1080
};

```

## 2. 切片继承 (Slice Inheritance)

本语言支持一种受限的单继承模型，称为“切片继承”。其核心目的不是为了面向对象的层级抽象，而是为了**内存布局的复用**和**指针切片 (Pointer Slicing)**。

### 2.1 语法与语义

* **语法：** `struct Child: Parent { ... }`
* **约束：**
* 仅支持单继承。
* 父结构体（Parent）的所有字段会被**平铺 (Flattened)** 在子结构体（Child）的内存起始位置。
* **布局保证：** 指向 `Child` 的指针地址与指向其内部 `Parent` 部分的指针地址严格一致。

### 2.2 内存模型示意

假设有以下定义：

```loom
struct Header {
    id: u16,
    len: u16
}

struct Packet: Header {
    payload: u64
}

```

内存布局如下：

```text
+------- Packet (Total Size) -------+
| [Header (Base)] |  Packet Fields  |
| id (u16)        |                 |
| len (u16)       |  payload (u64)  |
+-----------------+-----------------+
^                 ^
|                 |
&Packet           &Packet.payload (偏移量为 Header 大小)
|
等价于 &Header

```

### 2.3 切片转换 (Slicing/Upcasting)

由于布局的确定性，可以使用 `as` 关键字安全地将子结构体转换为父结构体。

* **引用转换：** `&Child` as `&Parent`。这是一个零开销操作，仅仅是类型标签的改变，地址不变。
* **值转换：** `Child` as `Parent`。这会发生**对象切片 (Object Slicing)**，丢失子类的字段，仅保留父类部分的数据。

```loom
let pkt = Packet { id: 1, len: 8, payload: 0xFF };
let hdr_ptr: &Header = &pkt as &Header; // 安全，指向同一地址

```

## 3. 枚举 (Enum)

枚举支持代数数据类型 (ADT)，允许变体携带数据。Loom 的枚举设计强调内存布局的透明性和定义的扁平化。

### 3.1 基础类型控制

默认情况下，编译器自动选择能够容纳 Tag 的最小整数类型。用户也可以显式指定**基类型 (Backing Type)**。

* **语法：** `enum Name: BaseType { ... }`
* **用途：** 确保枚举 Tag 的大小与 C 库或硬件协议匹配。

```loom
// 强制 Tag 使用 u32
enum ErrorCode: u32 {
    NotFound = 404,  // 支持显式赋值
    Internal = 500,
    Timeout          // 自动递增
}

```

### 3.2 变体定义与约束

支持三种变体形式。为了保持定义清晰，**变体内部禁止定义嵌套的匿名结构体或枚举**。

1. **单元变体 (Unit Variant)：** 不带数据。
* `Stop`


2. **元组变体 (Tuple Variant)：** 带匿名数据。
* `Color(u8, u8, u8)`
* **限制：** 不支持默认值（避免位置参数歧义）。


3. **结构体变体 (Struct Variant)：** 带命名字段。
* `Move { x: i32, y: i32 }`
* **扁平化约束：** `{ ... }` 内部只能声明字段，**禁止**再嵌套定义 `struct` 或 `enum`。若需复杂结构，请先在外部定义 `struct`，再将其作为字段类型使用。



```loom
struct Payload {
    id: i32,
    data: [64]u8
}

enum Event {
    // 1. 单元变体
    Quit,
    
    // 2. 元组变体 (不支持默认值)
    Click(i32, i32), 
    
    // 3. 结构体变体
    // 支持默认值，语法同 Struct
    Key { 
        code: i32, 
        repeated: bool = false // 默认值
    },
    
    // 4. 引用外部结构体 (正确做法)
    Message { 
        p: Payload 
    }
    
    // ❌ 错误：禁止内联嵌套定义
    // Bad { nested: struct { a: i32 } } 
}

```

### 3.3 初始化与默认值

枚举的初始化采用**点号语法 (`.Variant`)**。对于结构体变体，如果字段定义了默认值，初始化时可以省略该字段。

```loom
// 1. 初始化单元变体
let e1 = Event.Quit;

// 2. 初始化元组变体 (必须提供所有值)
let e2 = Event.Click(100, 200);

// 3. 初始化结构体变体 (利用默认值)
// repeated 默认为 false
let e3 = Event.Key { code: 13 }; 

// 显式覆盖默认值
let e4 = Event.Key { code: 13, repeated: true };

```

## 4. 联合体 (Union)

提供 C 风格的联合体，用于底层内存操作。所有字段共享同一块内存起始位置。

* **语法：** `union Data { i: i32, f: f32 }`
* **安全性：** 访问 Union 的字段虽然不是 UB，但值的语义完全取决于程序员如何解释。
* **大小：** 等于最大字段的大小。

## 5. 命名空间与作用域 (Namespaces & Scopes)

在 Loom 中，`struct`、`enum` 和 `union` 的定义块不仅仅描述数据布局，它们本质上还开启了一个以类型名为标识符的 **命名空间 (Namespace)**。

### 5.1 静态成员与嵌套

你可以在类型定义块内部定义常量、嵌套类型（Type Alias 或其他数据结构）以及静态函数。这些成员必须通过类型名进行访问。

* **作用域：** 定义块内部可以直接访问同级成员。
* **泛型引入：** 如果类型定义了泛型参数（如 `<T>`），该参数在整个定义块内部可见。

```loom
struct Buffer<T> {
    // 1. 数据字段 (在此处可以使用泛型 T)
    ptr: *T,
    len: usize,

    // 2. 静态常量
    // 外部访问：Buffer.DEFAULT_CAP
    pub const DEFAULT_CAP: usize = 16;

    // 3. 嵌套类型
    // 外部访问：Buffer.Error
    pub enum Error { Overflow, Empty }

    // 4. 静态工厂方法 (不含 self)
    // 外部访问：Buffer.new(...)
    // 注意：在此处可以直接使用 T，无需重新声明
    pub fn new(size: usize) Buffer<T> {
        // ...
    }
}

```

### 5.2 泛型作用域

当定义泛型结构体时，泛型参数 `<T>` 被视为引入到了当前的大括号作用域中。

* **语法：** `struct Name<T> { ... }`
* **约束简述：** 如果需要对 `T` 施加约束（例如 `T: Copy`），建议在泛型定义处直接标注（详见 **[泛型]** 章节）。

```loom
// T 被引入作用域，字段 x 和 y 均可使用
struct Point<T> {
    x: T,
    y: T
}

// 即使在静态上下文中，也能感知到 T 的存在
// (注：具体泛型方法的实例化规则见后续章节)

```

# 模块与导入 (Modules & Imports)

本语言采用基于文件系统的模块化组织方式。源码文件结构直接映射为逻辑模块结构，无需显式的 `module` 或 `namespace` 声明块。

## 1. 模块结构 (Module Structure)

编译器将每个源码文件（`.loom`）视为一个独立的模块。文件系统的目录层级定义了模块的层级关系。

### 1.1 文件模块 (File Modules)

最基础的模块单元。

* **规则：** 文件名即模块名。
* **示例：**
* 文件 `math.loom` 定义了模块 `math`。
* 文件 `utils/string.loom` 定义了模块 `utils.string`（前提是 `utils` 目录被识别为父模块）。



### 1.2 目录模块 (Directory Modules)

包含 `init.loom` 的目录被视为一个包含子模块的父模块。

* **规则：** 目录下的 `init.loom` 是该目录模块的入口点（Entry Point）。
* **作用：** 用于重新导出（Re-export）子模块的内容，或定义该目录层级共用的逻辑。
* **结构示例：**
```text
src/
├── main.loom        // 根模块 (root)
├── net/             // 模块: net
│   ├── init.loom    // net 模块的代码主体
│   ├── http.loom    // 模块: net.http
│   └── tcp.loom     // 模块: net.tcp

```



## 2. 可见性控制 (Visibility)

默认情况下，模块内定义的所有符号（函数、结构体、变量等）均是 **私有 (Private)** 的，仅当前文件可见。

### 2.1 导出 (pub)

使用 `pub` 关键字标记的符号可被外部模块访问。

```loom
// math.loom

// 私有函数，外部无法访问
fn helper() { ... }

// 公开函数，外部可见
pub fn add(a: i32, b: i32)  i32 { ... }

// 公开结构体
pub struct Vector {
    pub x: i32, // 字段也需要显式 pub
    y: i32,     // 私有字段
}

```

## 3. 导入 (Imports)

使用 `use` 关键字引入其他模块的符号。导入路径分为 **绝对路径** 和 **相对路径**。

### 3.1 绝对路径导入 (Absolute Imports)

不以点号（`.`）开头的路径被视为绝对路径。

* **解析顺序：**
1. **预定义符号/标准库：** 编译器检查编译选项或环境预置的根命名空间（如 `std`）。
* `use std.debug.print;`


2. **项目根 (Root)：** 如果未找到预定义符号，则从项目源代码根目录（Project Root）开始查找。
* `use math.PI;` 等价于 `use root.math.PI;`

### 3.2 相对路径导入 (Relative Imports)

以点号（`.`）开头的路径基于当前文件所在的位置进行解析。

* `use .submodule;` —— 导入当前目录下的 `submodule`。
* `use ..utils;` —— 导入上一级目录中的 `utils`。

> **示例场景：**
> 在 `src/net/http.loom` 中：
> * `use .tcp;` 指向 `src/net/tcp.loom`
> * `use ..main;` 指向 `src/main.loom`
> 
> 

### 3.3 组合导入与通配符

为了减少代码冗余，支持在一行内导入多个符号。

* **分组导入 (Grouping):** 使用 `{}` 包裹同一模块下的多个符号。
```loom
use math.{add, sub, PI};
use std.io.{File, Reader as IoReader}; // 支持重命名

```

* **通配符导入 (Glob Import):** 使用 `*` 导入该模块下所有 `pub` 可见的符号。
* *注意：不建议在库开发中过度使用，以免污染命名空间。*

```loom
use http.*;

```

### 3.4 重新导出 (Re-export)

模块可以通过 `pub use` 将引入的符号再次导出，使其成为当前模块公共 API 的一部分。这常用于平铺复杂的内部目录结构（Facade Pattern）。

```loom
// 文件: net/init.loom
// 将内部具体实现的符号导出到 net 模块下
pub use .http.Request;
pub use .http.Response;

```

此时可以使用 `use net.Request;` 而无需知道 `http` 子模块的存在。

# 行为与多态 (Behavior & Polymorphism)

本章节描述如何为数据定义行为（方法），以及如何通过 Trait 定义接口契约。

Loom 严格区分 **类型的定义（包含静态命名空间）** 与 **行为的实现（包含实例方法）**。

## 1. 关键字：`self` 与 `Self`

在定义行为时，理解这两个关键字的区别至关重要：

| 关键字 | 作用域                            | 含义               | 用途                                     |
| ------ | --------------------------------- | ------------------ | ---------------------------------------- |
| `Self` | `struct`, `enum`, `impl`, `trait` | **当前类型的别名** | 用于返回值类型、泛型约束、内部类型引用   |
| `self` | **仅限 `impl**`                   | **当前实例的值**   | 表示方法接收者（Receiver），即“对象本身” |

> **规则：** `self` 只能出现在 `impl` 块的方法参数列表中。严禁在 `struct` 或 `enum` 的定义块中使用 `self`。

## 2. 方法实现 (Implementation)

我们使用 `impl` 关键字来为类型添加**实例方法 (Instance Methods)**。

### 2.1 通用实现语法 (Universal Implementation Syntax)

Loom 的 `impl` 机制没有针对指针的特殊规则。它遵循单一且通用的法则：**`impl T` 为类型 `T` 定义实例方法。**

这里的 `T` 可以是任何合法的类型，包括结构体、基本类型、引用、甚至原生指针。**类型即上下文**——你为哪个类型实现方法，`self` 就是那个类型的值。

* **语法：** `impl Type { ... }`
* **Self 推导：** 在块内部，`self` 的类型严格等同于 `Type`。

```loom
struct Circle { radius: i32 }

// 1. 扩展结构体本身 (Value Context)
// 场景：消耗所有权，或纯计算（Copy语义）
impl Circle {
    // self: Circle
    fn area(self) i32 {
        3 * self.radius * self.radius
    }
}

// 2. 扩展可变引用 (Mutation Context)
// 场景：修改状态
// 这不是什么"引用方法"，这只是为 "&mut Circle" 这个类型定义了方法
impl &mut Circle {
    // self: &mut Circle
    fn grow(self, size: i32) {
        // 直接访问字段，因为 &mut T 允许解引用访问
        self.radius += size;
    }
}

// 3. 扩展原生指针 (Raw Context)
// 场景：底层操作，空检查
// Loom 允许直接为 *T 定义方法，极大提升了底层代码的可读性
impl *Circle {
    // self: *Circle
    fn is_valid(self) bool {
        self != null and self.radius > 0
    }
}

// 使用示例
let mut c = Circle { radius: 10 };

c.area();           // 匹配 impl Circle
(&mut c).grow(5);   // 匹配 impl &mut Circle

let ptr: *Circle = &c as *Circle;
ptr.is_valid();     // 匹配 impl *Circle

```
### 2.2 内部辅助函数 (Internal Helpers)

由于 `impl` 块不是静态命名空间，因此：

* **不能**通过 `Type.method()` 的方式从外部调用 `impl` 块中定义的函数。
* **但是**，可以在 `impl` 内部定义不带 `self` 的函数，作为**私有辅助函数**，仅供该 `impl` 块内部的其他方法调用。

> **编译器警告：** 如果你在 `impl` 块中定义了一个 `pub` 的静态函数（不带 `self`），编译器会发出警告，因为该函数在外部是**不可达 (Unreachable)** 的。

```loom
impl &mut Circle {
    // 这是一个内部辅助函数，外部无法访问
    // 即使加了 pub 也没用，因为你无法写出 &mut Circle.check_size() 这种语法
    fn _validate(r: i32) bool {
        r > 0
    }

    pub fn set_radius(self, r: i32) {
        // 在内部可以调用辅助函数
        if _validate(r) {
            self.radius = r;
        }
    }
}

```

## 3. 特质 (Traits)

Trait 定义了一组行为契约。

### 3.1 定义 Trait

Trait 定义中可以使用 `Self` 指代实现者类型。

```loom
trait Shape {
    // 抽象方法
    fn area(self) i32; 
    
    // 默认方法
    fn describe(self) {
        print("I am a shape of size:", self.area());
    }
}

```

### 3.2 实现 Trait

使用 `impl Type: Trait` 语法。这里同样遵循“类型即上下文”原则，必须明确是为 `T`、`&T` 还是 `&mut T` 实现 Trait。

```loom
// 为 &Circle 实现 Shape
// 意味着只有 Circle 的引用才具备 Shape 的行为
impl &Circle: Shape {
    fn area(self) i32 {
        3 * self.radius * self.radius
    }
}

fn main() {
    let c = Circle { radius: 10 };
    let ptr = &c;
    
    // 调用
    ptr.describe(); 
}

```

## 4. 匿名类型的扩展

Loom 的设计允许为**任意类型**编写 `impl`，包括匿名类型（如元组或定长数组）。

由于这些类型没有 `struct` 定义块，它们天然没有静态命名空间（你无法定义 `(u8, i32).new()`）。但是，通过 `impl`，你可以赋予它们行为。

```loom
// 为特定元组添加方法
impl (i32, i32) {
    fn sum(self) i32 {
        self[0] + self[1]
    }
}

fn main() {
    let pair = (10, 20);
    // 合法调用
    let s = pair.sum(); 
}

```

## 3. 泛型 (Generics)

泛型允许编写参数化多态的代码。支持在结构体、枚举、Trait 和 `impl` 块中使用。

### 3.1 泛型定义

使用尖括号 `<T>` 声明类型参数。

```loom
// 泛型结构体
struct Box<T> {
    item: T
}

// 泛型 Trait
trait Converter<From, To> {
    fn convert(From) To;
}

```

### 3.2 泛型实现 (Generic Impl)

在 `impl` 块中使用泛型时，必须先在 `impl` 关键字后声明泛型参数。

```loom
// 为任意类型的 Box 实现方法
impl <T> Box<T> {
    fn new(item: T) Box<T> {
        Box { item: item }
    }
}

// 带有 Trait 约束的泛型 
impl <T: Display> &mut Box<T>: Display {
    fn show(self) {
        self.item.print(); // 假设 T 有 print 方法
    }
}

```

#### 4. 泛型语法与上下文 (Generics Syntax & Context)

由于 `<` 和 `>` 符号同时用于泛型定义和比较运算符，为了消除解析歧义（Turbofish 问题），Loom 在不同上下文中对泛型参数的写法有不同要求。

##### 4.1 类型上下文 (Type Context)

在明确需要类型的地方（如变量声明、函数签名），直接使用 `<T>`。

```loom
// 此时解析器明确知道这里需要类型
let list: List<i32> = ...;
fn process(val: Option<str>) { ... }

```

##### 4.2 表达式上下文 (Expression Context)

在表达式中（如函数调用、结构体实例化），必须使用 `.<T>` 语法。这明确告知解析器“接下来是泛型参数”而不是“小于号”。

```loom
// 错误写法 (解析器会认为 List 小于 i32)
// let l = List<i32>.new(); 

// 正确写法
let l = List.<i32>.new();
let v = parse_value.<f64>("3.14");

```

#### 5. 类型推导与省略模式 (Type Inference & Dot Syntax)

Loom 拥有强大的上下文类型推导能力。当编译器能够根据上下文（如函数返回值、变量类型标注）确定目标类型时，开发者可以省略具体的类型名称，直接使用点号 `.` 开头。

这在处理 **泛型枚举 (Generic Enum)** 和 **错误处理** 时尤为高效。

##### 5.1 基础推导

如果目标类型已知是某个 Enum，可以直接写 `.Variant`。

```loom
enum Color { Red, Green, Blue }

// 显式类型标注，右侧可省略 Color
let c: Color = .Red; 

fn set_color(c: Color) { ... }
// 参数位置推导
set_color(.Green);

```

##### 5.2 泛型与嵌套推导 (Advanced Inference)

此特性对于 `Result<T, E>` 等复杂泛型类型同样适用。编译器会自动推导泛型参数，开发者无需手写冗长的 `Result.<File, FileError>.Err(...)`。

**场景示例：**

```loom
// 定义
enum Result<T, E> { Ok(T), Err(E) }
enum FileError { NotFound, PermissionDenied }

fn strict_open(path: &str) Result<File, FileError> {
    if !exists(path) {
        // 1. 推导外层：返回类型已知是 Result<File, FileError>
        //    所以 .Err 被推导为 Result.Err
        // 2. 推导内层：Err 需要 FileError 类型
        //    所以 .NotFound 被推导为 FileError.NotFound
        return .Err(.NotFound);
    }

    // 同理，.Ok 自动推导 T 为 File
    .Ok(get_file(path))
}

```

这种写法极大地减少了代码噪声，使得错误处理逻辑更加清晰，专注于数据流本身而非类型标注。


### 3.5 泛型作用域与命名空间 (Generic Scope)

当定义泛型类型时（如 `struct Point<T>`），泛型参数 `T` 不仅仅用于描述字段类型，它还被引入到了该类型的 **静态命名空间** 中。

这意味着在 `struct/enum` 定义块内部的静态方法、常量或嵌套类型，均可直接引用该泛型参数，而无需重新声明。

```loom
struct Container<T> {
    // T 用于字段
    data: T,

    // T 用于静态工厂方法 (无需在 fn 后再写 <T>)
    // 此时的 T 指代的是 Container<T> 中的那个 T
    pub fn new(val: T) Container<T> {
        Container { data: val }
    }
    
    // T 用于内部嵌套类型
    struct Iterator {
        ptr: *T,
        count: usize
    }
}

fn main() {
    // 实例化时，i32 被传入 Container 的命名空间
    // 因此 Container.<i32>.new 中的 T 变成了 i32
    let c = Container.<i32>.new(100);
}

```

# 特质与运算符重载 (Traits & Operator Overloading)

Trait 定义了类型必须具备的行为契约。在 Loom 中，Trait 不仅用于泛型约束（静态分发），也是实现运行时多态（动态分发）和运算符重载的基础。

## 1. 运算符重载 (Operator Overloading)

Loom 不支持随意的自定义运算符，而是通过实现内置 Trait 来重载标准运算符的行为。

### 1.1 算术与比较 Trait

编译器将运算符语法糖映射为特定的 Trait 方法调用。

| 运算符   | 对应 Trait | 方法签名                        |
| -------- | ---------- | ------------------------------- |
| `a + b`  | `Add<Rhs>` | `fn add(self, rhs: Rhs) Output` |
| `a - b`  | `Sub<Rhs>` | `fn sub(self, rhs: Rhs) Output` |
| `a * b`  | `Mul<Rhs>` | `fn mul(self, rhs: Rhs) Output` |
| `a / b`  | `Div<Rhs>` | `fn div(self, rhs: Rhs) Output` |
| `a == b` | `Eq<Rhs>`  | `fn eq(self, rhs: Rhs) bool`    |
| `!a`     | `Not`      | `fn not(self) Output`           |
| `#a`     | `Len`      | `fn len(self) usize`            |

> **注：** 逻辑运算符 `and` 和 `or` 具备短路特性，不可重载。

### 1.2 实现示例

```loom
struct Vector2 { x: i32, y: i32 }

// 实现 Add Trait
// 这里的 self 是按值传递（消耗所有权），若需引用相加需实现 impl &Vector2: Add
impl Vector2: Add<Vector2> {
    type Output = Vector2; // 关联类型，指定结果类型

    fn add(self, other: Vector2) Vector2 {
        Vector2 {
            x: self.x + other.x,
            y: self.y + other.y
        }
    }
}

// 使用
let v1 = Vector2 { x: 1, y: 2 };
let v2 = Vector2 { x: 3, y: 4 };
let v3 = v1 + v2; // 自动调用 Add.add

```

## 2. 特质对象 (Trait Objects)

当需要在运行时处理不同类型但具备相同行为的对象时，Loom 使用特质对象进行动态分发。

### 2.1 引用类型 (`&Trait`)

Trait 本身可以作为一种类型使用（Existent Type），但必须通过引用或指针形式存在。

* `&Trait`: 不可变特质对象。
* `&mut Trait`: 可变特质对象。
* **内存布局：** 特质对象是一个 **胖指针 (Fat Pointer)**，包含两个机器字：
1. **data:** 指向具体数据的指针。
2. **vtable:** 指向虚函数表（VTable）的指针。



### 2.2 创建与转换 (`as`)

具体类型转换为特质对象必须使用 `as` 显式转换，或者在变量绑定时显式标注类型。

```loom
trait Drawable {
    fn draw(self);
}

struct Button { ... }
impl &Button: Drawable { ... }

struct Slider { ... }
impl &Slider: Drawable { ... }

fn main() {
    let btn = Button { ... };
    
    // 方式 1: 显式 as 转换
    let obj: &Drawable = &btn as &Drawable;
    
    // 方式 2: 绑定转换 (Coercion at binding)
    let components: []&Drawable = [
        &btn as &Drawable,
        &Slider { ... } as &Drawable
    ];

    for c in components {
        c.draw(); // 动态分发：通过 vtable 调用对应的 implementation
    }
}

```

## 3. 特质继承 (Trait Inheritance)

Loom 支持强大的特质继承机制，允许定义特质之间的依赖关系。这形成了一个行为的层级结构。

### 3.1 基础继承

子特质（Sub-trait）要求实现者必须同时实现父特质（Super-trait）。

* **语法：** `trait Child: Parent { ... }`

```loom
trait ToString {
    fn to_string(self) String;
}

// 任何实现 Display 的类型，必须先实现 ToString
trait Display: ToString {
    fn print(self) {
        // 可以安全调用父特质的方法
        sys.out(self.to_string());
    }
}

```

### 3.2 泛型继承与多重约束

这是 Loom 的强项。Trait 继承可以携带泛型参数，允许表达复杂的类型约束关系。

* **语法：** `trait Child<T>: Parent<T> + OtherTrait { ... }`

```loom
// 定义一个图节点的特质
trait Node { fn id(self) i32; }

// 定义边，E 代表边的权重类型
trait Edge<W> { fn weight(self) W; }

// 定义图，N 是节点类型，E 是边类型
// 要求：
// 1. 图必须是可迭代的 (Iterable)
// 2. 节点 N 必须实现 Node 特质
// 3. 边 E 必须实现 Edge 特质
trait Graph<N: Node, E: Edge<f32>>: Iterable<N> 
{
    fn add_node(self, node: N);
    fn get_edge(self, from: N, to: N) ?E;
}

```

在编译器内部，这种继承关系保证了当你在泛型环境中使用 `T: Graph<N, E>` 时，可以直接调用 `Iterable` 的方法，无需重复声明。


# 错误处理 (Error Handling)

Loom **不提供** 内置的统一错误处理机制（如 try-catch 或强制的 Result 类型）。
Loom 遵循 **“机制与策略分离”** 的原则：语言提供基础工具（`?T`, `enum`, `match`, `macro`），由开发者根据项目需求决定错误处理策略。

## 1. 策略一：简单传播 (Simple Propagation)

对于只关心“成功”或“失败”，而不关心“失败原因”的场景（如脚本编写、快速原型），直接使用 **可空类型 (`?T`)**。

* **机制：** 利用 `?T`, `null`, `.?` 和 `?` 操作符。
* **特点：** 代码极其简洁，类似 Shell 脚本的 `set -e`。

```loom
fn read_config() ?Config {
    // 如果 open 失败返回 null，.? 会让整个函数立即返回 null
    let file = File.open("config.ini").?; 
    let data = file.read_all().?;
    parse(data) // 如果 parse 返回 ?Config
}

```

## 2. 策略二：富错误处理 (Rich Error Handling)

对于系统级编程或库开发，需要向调用者报告具体的错误原因。此时应使用 **ADT (Enum)** 模式。

* **机制：** 自定义泛型枚举（通常命名为 `Result`）。
* **特点：** 类型安全，强制处理所有错误路径。

```loom
// 用户自定义的 Result 类型（标准库通常会提供一个）
enum Result<T, E> {
    Ok(T),
    Err(E)
}

// 具体的错误枚举
enum FileError { NotFound, PermissionDenied, DiskFull }

fn strict_open(path: &str) Result<File, FileError> {
    if !exists(path) {
        return .Err(.NotFound);
    }
    // ...
    .Ok(file)
}

// 调用方必须处理错误
match strict_open("test.txt") {
    .Ok(f) => process(f),
    .Err(e) => log_error(e)
}

```

## 3. 策略三：宏辅助 DSL

开发者可以利用 Loom 的宏系统，为自定义的 `Result` 类型封装类似于 Rust `?` 操作符的语法糖。

```loom
// 假设定义了一个 try! 宏，用于解包 Result 或 return Err
macro try {
    ($expr:expr) => {
        match $expr {
            .Ok(val) => val,
            .Err(e) => return .Err(e)
        }
    }
}

// 使用
let f = try!(strict_open("test.txt"));

```

# 宏系统

本语言提供了一套基于 **模式匹配（Pattern Matching）** 的声明式宏系统。宏在编译的 **AST 展开阶段** 执行，允许用户在代码生成前操作语法树。

与许多语言不同，本语言的宏并非全局存在，而是严格遵循 **模块与命名空间（Namespacing）** 规则，必须导入后方可使用。

## 1. 宏定义 (Definition)

宏定义使用 `macro` 关键字。其核心机制是“匹配-替换”：根据传入的 Token 流匹配定义的模式，并生成相应的 AST 片段。

**语法结构：**

```loom
macro <Name> {
    (<Matcher>) => { <Expansion> };
    (<Matcher>) => { <Expansion> };
}

```

### 1.1 捕获器 (Captures)

宏参数通过 `$` 前缀捕获，需指定片段类型（Fragment Specifier）。

| 片段类型   | 描述     | 示例              |
| ---------- | -------- | ----------------- |
| `$x:expr`  | 表达式   | `1 + 2`, `func()` |
| `$x:ident` | 标识符   | `variable_name`   |
| `$x:type`  | 类型描述 | `i32`, `&mut T`   |
| `$x:block` | 代码块   | `{ ... }`         |
| `$x:stmt`  | 语句     | `let x = 1;`      |

**示例：实现一个类似于 `if` 的宏**

```loom
macro my_if {
    ($cond:expr, $body:block) => {
        match $cond {
            true => $body,
            false => {}
        }
    }
}

```

### 1.2 重复模式 (Repetition)

支持类似正则的重复匹配，用于处理变长参数。

* `$(...)*` : 0 次或多次
* `$(...)+` : 1 次或多次
* `$(...),*` : 以逗号分隔

```loom
// 使用指定分配器创建列表
macro vec {
    // 模式：先捕获 allocator，再捕获后续的元素列表
    ( $alloc:expr, $( $elem:expr ),* ) => {
        {
            // 假设 ArrayList.init 需要传入 allocator
            let mut list = ArrayList.init($alloc); 
            $(
                list.push($elem);
            )*
            list
        }
    }
}

// 使用示例
// 假设 gpa 是一个通用分配器
let numbers = vec![gpa, 10, 20, 30];

```

## 2. 宏调用与命名空间 (Invocation & Namespacing)

宏的调用在语法上被视为 **后缀操作**。这使得宏可以像普通标识符一样存在于模块路径中。

### 2.1 语法设计

宏调用的标记是后缀 `!`。

* **语法：** `path::to::macro_name!(args)`
* 解析器将其视为：`Expr(Path(path::to::macro_name), SuffixOp(!), Args(args))`。

### 2.2 模块化规则

宏不污染全局命名空间。要使用其他模块的宏，必须显式引用或 `use`。

```loom
// 定义在 std::io 模块中
// macro println { ... }

// 使用方式 1：全路径调用
std.io.println!("Hello");

// 使用方式 2：导入后调用
use std.io.println;
println!("World");

```

## 3. 卫生性 (Hygiene)

宏是 **部分卫生 (Partially Hygienic)** 的：

* **局部变量安全：** 宏内部定义的 `let` 变量通常会被编译器重命名，避免与外部作用域冲突。
* **显式捕获：** 传入宏的标识符（`$x:ident`）保持其原本的上下文引用。

这是一个非常现代化且直观的模块系统设计，结合了 Python 的文件系统映射直觉和 Rust 的显式可见性控制。去掉 `mod {}` 块，直接让文件即模块，能大大降低认知负担。

以下是为您规划的 **模块与导入 (Modules & Imports)** 章节。我着重区分了“模块定义”与“路径解析逻辑”，这对编译器实现和用户理解都至关重要。

---



# 外部函数接口 (FFI & Interoperability)

Loom 将与 C 语言的互操作视为一等公民。`extern` 关键字不仅用于导入符号，还用于控制内存布局和调用约定。

## 1. 外部布局 (Extern Layouts)

默认情况下，Loom 编译器会对 `struct` 字段进行重排以优化内存。若需与 C 代码或硬件寄存器交互，必须使用 `extern` 标记。

* **语法：** `extern struct Name { ... }`
* **语义：**
* **C 兼容布局：** 编译器严格按照字段声明顺序进行布局。
* **对齐规则：** 遵循目标平台的 C ABI 对齐规则。
* **用途：** 用于定义系统 API 结构体、硬件映射或跨语言共享数据结构。



```loom
// 内存布局与 C 的 struct Point { int x; int y; } 完全一致
extern struct Point {
    x: i32,
    y: i32,
}

```

## 2. 外部函数与变长参数 (Extern Functions)

`extern` 也可以修饰函数声明，用于描述 C 函数签名，特别是解锁了变长参数（Variadic Arguments）的能力。

* **语法：** `extern fn name(args, ...) ret;`
* **变长参数 (`...`)：**
* 仅允许在 `extern fn` 中使用。
* 对应 C 语言的 `va_list` 机制。
* **安全性：** 调用变长参数函数是**不安全**的（但在 Loom 中不强制 `unsafe` 块，需程序员自负盈亏），编译器无法检查参数类型。



```loom
extern {
    // 导入 libc 的 printf
    fn printf(fmt: *u8, ...) i32;
    fn malloc(size: usize) *mut u8;
    fn free(ptr: *mut u8);
}

fn main() {
    let msg = "Hello C World\n";
    // 直接调用，如同原生函数
    printf(&msg[0], 10, 20); 
}

```



