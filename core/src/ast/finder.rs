use crate::ast::{
    Block, Expression, ExpressionData, Program, TableDefinition, TableItem, TopLevelItem, TypeRef,
    TypeRefData,
};
use crate::utils::Span;

/// 我们在 AST 中找到了什么？
pub enum AstNode<'a> {
    Expression(&'a Expression),
    TypeRef(&'a TypeRef),
    // 将来可能还需要 FieldDefinition 或 MethodDefinition，但目前跳转定义主要靠前两个
}

/// 入口函数：在 Program 中查找包含 offset 的最深层节点
pub fn find_node_at_offset(program: &Program, offset: usize) -> Option<AstNode> {
    // 1. 先检查是否在整个程序范围内 (稍微放宽一点范围，或者不做检查直接遍历顶层)
    if !program.span.contains(offset) {
        // 可选：如果不在范围内直接返回 None，
        // 但通常顶层 items 之间可能有空白，所以还是遍历 items 比较保险
    }

    for item in &program.definitions {
        match item {
            TopLevelItem::Table(table) => {
                if table.span.contains(offset) {
                    return find_in_table(table, offset);
                }
            }
            TopLevelItem::Use(stmt) => {
                // 如果需要支持跳转到 import 的文件，这里以后处理
                if stmt.span.contains(offset) {
                    // return Some(...)
                }
            }
        }
    }
    None
}

fn find_in_table(table: &TableDefinition, offset: usize) -> Option<AstNode> {
    // 1. 检查是否在父类声明上 [Dog : Animal]
    if let Some(proto) = &table.data.prototype {
        if proto.span.contains(offset) {
            return find_in_type_ref(proto, offset);
        }
    }

    // 2. 检查泛型参数 <T: Constraint>
    for generic in &table.data.generics {
        if let Some(constraint) = &generic.data.constraint {
            if constraint.span.contains(offset) {
                return find_in_type_ref(constraint, offset);
            }
        }
    }

    // 3. 检查表内条目
    for item in &table.data.items {
        match item {
            TableItem::Field(field) => {
                if field.span.contains(offset) {
                    // 检查类型标注
                    if let Some(ty) = &field.data.type_annotation {
                        if ty.span.contains(offset) {
                            return find_in_type_ref(ty, offset);
                        }
                    }
                    // 检查初始值表达式
                    if let Some(expr) = &field.data.value {
                        if expr.span.contains(offset) {
                            return find_in_expression(expr, offset);
                        }
                    }
                }
            }
            TableItem::Method(method) => {
                if method.span.contains(offset) {
                    // 检查参数类型
                    for param in &method.data.params {
                        if param.data.type_annotation.span.contains(offset) {
                            return find_in_type_ref(&param.data.type_annotation, offset);
                        }
                    }
                    // 检查返回值类型
                    if let Some(ret) = &method.data.return_type {
                        if ret.span.contains(offset) {
                            return find_in_type_ref(ret, offset);
                        }
                    }
                    // 检查方法体
                    if let Some(body) = &method.data.body {
                        if body.span.contains(offset) {
                            return find_in_block(body, offset);
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_in_block(block: &Block, offset: usize) -> Option<AstNode> {
    for stmt in &block.data.statements {
        if stmt.span.contains(offset) {
            return find_in_expression(stmt, offset);
        }
    }
    None
}

fn find_in_type_ref(ty: &TypeRef, offset: usize) -> Option<AstNode> {
    // 如果光标在这个 TypeRef 范围内，看看能不能钻得更深
    match &ty.data {
        TypeRefData::Named(_) => {
            // 这是叶子节点 (比如 "int" 或 "User")
            Some(AstNode::TypeRef(ty))
        }
        TypeRefData::GenericInstance { base: _, args } => {
            // 比如 List<int>，如果光标在 int 上
            for arg in args {
                if arg.span.contains(offset) {
                    return find_in_type_ref(arg, offset);
                }
            }
            // 如果不在参数里，那就在 List 上
            Some(AstNode::TypeRef(ty))
        }
        TypeRefData::Structural(params) => {
            for param in params {
                if param.data.type_annotation.span.contains(offset) {
                    return find_in_type_ref(&param.data.type_annotation, offset);
                }
            }
            Some(AstNode::TypeRef(ty))
        }
    }
}

fn find_in_expression(expr: &Expression, offset: usize) -> Option<AstNode> {
    // 递归查找表达式树
    match &expr.data {
        ExpressionData::Identifier(_) => {
            // 找到了变量使用
            Some(AstNode::Expression(expr))
        }
        ExpressionData::FieldAccess { target, field: _ } => {
            // 如果是 target.field，先看是不是在 target 里
            if target.span.contains(offset) {
                return find_in_expression(target, offset);
            }
            // 如果不是 target，那可能是在 field 上
            // 这里的 field 是 Symbol，通常没有单独的 Span (除非 Symbol 携带 Span)
            // 但整个 Expression 的 span 包含了 field。
            // 如果 target 不包含 offset，且 expression 包含 offset，那就是 field。
            Some(AstNode::Expression(expr))
        }
        ExpressionData::Binary { left, right, .. } => {
            if left.span.contains(offset) {
                find_in_expression(left, offset)
            } else if right.span.contains(offset) {
                find_in_expression(right, offset)
            } else {
                None
            }
        }
        ExpressionData::Unary { expr: inner, .. } => find_in_expression(inner, offset),
        ExpressionData::If {
            condition,
            then_block,
            else_block,
        } => {
            if condition.span.contains(offset) {
                return find_in_expression(condition, offset);
            }
            if then_block.span.contains(offset) {
                return find_in_block(then_block, offset);
            }
            if let Some(else_b) = else_block {
                if else_b.span.contains(offset) {
                    return find_in_block(else_b, offset);
                }
            }
            None
        }
        ExpressionData::Call {
            callee,
            generic_args,
            args,
        } => {
            if callee.span.contains(offset) {
                return find_in_expression(callee, offset);
            }
            for arg in args {
                if arg.span.contains(offset) {
                    return find_in_expression(&arg.data.value, offset);
                }
            }
            for g_arg in generic_args {
                if g_arg.span.contains(offset) {
                    return find_in_type_ref(g_arg, offset);
                }
            }
            None
        }
        ExpressionData::VariableDefinition { ty, init, .. } => {
            if let Some(t) = ty {
                if t.span.contains(offset) {
                    return find_in_type_ref(t, offset);
                }
            }
            if init.span.contains(offset) {
                return find_in_expression(init, offset);
            }
            // 如果光标在变量名上，这里暂时不返回 Def，因为 Goto Definition 通常是去往定义处，而不是在定义处原地跳
            None
        }
        ExpressionData::Assign { target, value, .. } => {
            if target.span.contains(offset) {
                find_in_expression(target, offset)
            } else if value.span.contains(offset) {
                find_in_expression(value, offset)
            } else {
                None
            }
        }
        ExpressionData::Block(b) => find_in_block(b, offset),
        // ... 其他类型 (Literal, Range, etc) 如果没有子节点，就返回 None (或者自身)
        // Literal 通常不需要跳转，所以返回 None
        _ => None,
    }
}
