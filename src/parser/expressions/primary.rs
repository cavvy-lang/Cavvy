//! 基本表达式解析
//!
//! 处理字面量、标识符、括号表达式、new表达式等基本表达式。

use crate::ast::*;
use crate::types::Type;
use crate::error::cayResult;
use super::super::Parser;
use super::super::types::is_type_token;
use super::lambda::try_parse_lambda;
use super::assignment::parse_expression;

/// 解析基本表达式
pub fn parse_primary(parser: &mut Parser) -> cayResult<Expr> {
    let loc = parser.current_loc();

    let token = parser.current_token().clone();
    match token {
        crate::lexer::Token::IntegerLiteral(Some((val, suffix))) => {
            parser.advance();
            let lit = match suffix {
                Some('L') | Some('l') => LiteralValue::Int64(val),
                None => {
                    // 默认整数字面量类型为 int32，但如果值超出范围，则视为 int64？
                    if val >= i32::MIN as i64 && val <= i32::MAX as i64 {
                        LiteralValue::Int32(val as i32)
                    } else {
                        LiteralValue::Int64(val)
                    }
                }
                _ => unreachable!(),
            };
            Ok(Expr::Literal(LiteralExpr { value: lit, loc }))
        }
        crate::lexer::Token::FloatLiteral(Some((val, suffix))) => {
            parser.advance();
            let lit = match suffix {
                Some('f') | Some('F') => LiteralValue::Float32(val as f32),
                Some('d') | Some('D') | None => LiteralValue::Float64(val),
                _ => unreachable!(),
            };
            Ok(Expr::Literal(LiteralExpr { value: lit, loc }))
        }
        crate::lexer::Token::StringLiteral(Some(s)) => {
            parser.advance();
            Ok(Expr::Literal(LiteralExpr { value: LiteralValue::String(s.clone()), loc }))
        }
        crate::lexer::Token::CharLiteral(Some(c)) => {
            parser.advance();
            Ok(Expr::Literal(LiteralExpr { value: LiteralValue::Char(c), loc }))
        }
        crate::lexer::Token::True => {
            parser.advance();
            Ok(Expr::Literal(LiteralExpr { value: LiteralValue::Bool(true), loc }))
        }
        crate::lexer::Token::False => {
            parser.advance();
            Ok(Expr::Literal(LiteralExpr { value: LiteralValue::Bool(false), loc }))
        }
        crate::lexer::Token::Null => {
            parser.advance();
            Ok(Expr::Literal(LiteralExpr { value: LiteralValue::Null, loc }))
        }
        crate::lexer::Token::This => {
            parser.advance();
            Ok(Expr::Identifier(IdentifierExpr {
                name: "this".to_string(),
                loc,
            }))
        }
        crate::lexer::Token::Super => {
            parser.advance();
            // super 可以作为标识符使用，用于 super.methodName() 调用
            Ok(Expr::Identifier(IdentifierExpr {
                name: "super".to_string(),
                loc,
            }))
        }
        crate::lexer::Token::Identifier(name) => {
            let name = name.clone();
            let loc = parser.current_loc();
            parser.advance();

            // 检查是否是方法引用或命名空间限定名: A::B 或 A::B::C
            if parser.check(&crate::lexer::Token::DoubleColon) {
                let save_pos = parser.pos;
                parser.advance(); // 消费 ::
                let second = parser.consume_identifier("Expected identifier after '::'")?;

                // 构建完整的命名空间限定名
                let mut qualified = format!("{}::{}", name, second);

                // 继续消费后续的 ::Identifier 段 (支持多层命名空间)
                while parser.check(&crate::lexer::Token::DoubleColon) {
                    parser.advance(); // 消费 ::
                    if let crate::lexer::Token::Identifier(_) = parser.current_token() {
                        let next = parser.consume_identifier("Expected identifier after '::'")?;
                        qualified = format!("{}::{}", qualified, next);
                    } else {
                        // :: 后不是标识符，回退整个命名空间解析
                        parser.pos = save_pos;
                        break;
                    }
                }

                // 判断最终语义:
                // - 如果下一个token是 . 或 ( → 命名空间限定类型名 (如 std::NetworkUtils.cleanup())
                // - 否则 → 方法引用 (如 String::valueOf, graphics::Circle::draw)
                if parser.pos != save_pos {
                    // 成功消费了 :: 段
                    if parser.check(&crate::lexer::Token::Dot) || parser.check(&crate::lexer::Token::LParen) {
                        // 命名空间限定名：用于后续 .member 或 (args) 的链式调用
                        return Ok(Expr::Identifier(IdentifierExpr { name: qualified, loc }));
                    }
                    // 方法引用：回退到第一次 :: 后，按 MethodRef 处理
                    parser.pos = save_pos;
                }
            }

            // 单层方法引用: ClassName::methodName (传统语法)
            if parser.match_token(&crate::lexer::Token::DoubleColon) {
                let method_name = parser.consume_identifier("Expected method name after '::'")?;
                return Ok(Expr::MethodRef(MethodRefExpr {
                    class_name: Some(name),
                    object: None,
                    method_name,
                    loc,
                }));
            }

            Ok(Expr::Identifier(IdentifierExpr { name, loc }))
        }
        // String 类型关键字也可以作为标识符使用（用于静态方法调用如 String.valueOf()）
        crate::lexer::Token::String => {
            parser.advance();

            // 检查是否是方法引用: String::methodName
            if parser.match_token(&crate::lexer::Token::DoubleColon) {
                let method_name = parser.consume_identifier("Expected method name after '::'")?;
                return Ok(Expr::MethodRef(MethodRefExpr {
                    class_name: Some("String".to_string()),
                    object: None,
                    method_name,
                    loc,
                }));
            }

            Ok(Expr::Identifier(IdentifierExpr { name: "String".to_string(), loc }))
        }
        crate::lexer::Token::New => {
            parser.advance();
            parse_new_expression(parser, loc)
        }
        crate::lexer::Token::LParen => {
            // 检查是否是 Lambda 表达式: (params) -> { body }
            // 需要向前看，检查是否有 -> 箭头
            let checkpoint = parser.pos;
            parser.advance(); // 跳过 '('

            // 尝试解析 Lambda 参数列表
            if let Ok(lambda_expr) = try_parse_lambda(parser, loc.clone()) {
                return Ok(lambda_expr);
            }

            // 不是 Lambda，回退并解析普通括号表达式
            parser.pos = checkpoint;
            parser.advance(); // 跳过 '('
            let expr = parse_expression(parser)?;
            parser.consume(&crate::lexer::Token::RParen, "期望 ')'\n提示: 括号表达式应以 ')' 结束，例如: (x + y)")?;
            Ok(expr)
        }
        crate::lexer::Token::LBrace => {
            // 数组初始化: {1, 2, 3}
            parser.advance(); // 跳过 '{'
            let mut elements = Vec::new();

            // 空数组初始化: {}
            if parser.check(&crate::lexer::Token::RBrace) {
                parser.advance();
                return Ok(Expr::ArrayInit(ArrayInitExpr { elements, loc }));
            }

            // 解析数组元素
            loop {
                elements.push(parse_expression(parser)?);
                if !parser.match_token(&crate::lexer::Token::Comma) {
                    break;
                }
                // 支持尾随逗号: {1, 2, 3,}
                if parser.check(&crate::lexer::Token::RBrace) {
                    break;
                }
            }

            parser.consume(&crate::lexer::Token::RBrace, "期望 '}'\n提示: 数组初始化器应以 '}' 结束，例如: {1, 2, 3}")?;
            Ok(Expr::ArrayInit(ArrayInitExpr { elements, loc }))
        }
        _ => {
            let current_token = parser.current_token();
            let (token_desc, suggestion) = match current_token {
                // 分隔符
                crate::lexer::Token::Semicolon => (
                    "分号(;)".to_string(),
                    "分号用于结束语句，不能作为表达式的开始。可能的问题:\n    - 前面缺少表达式，如: x = ; 应该是 x = 10;\n    - 多余的分号，如: if (x) ; { ... } 应该是 if (x) { ... }"
                ),
                crate::lexer::Token::RBrace => (
                    "右花括号(})".to_string(),
                    "右花括号用于结束代码块。可能的问题:\n    - 代码块内缺少语句或表达式\n    - 前面的语句缺少分号，导致解析器提前结束代码块"
                ),
                crate::lexer::Token::RBracket => (
                    "右方括号(])".to_string(),
                    "右方括号用于结束数组索引或类型声明。可能的问题:\n    - 数组索引前缺少数组对象，如: [0] 应该是 arr[0]\n    - 数组类型声明中缺少类型，如: [] arr 应该是 int[] arr"
                ),
                crate::lexer::Token::RParen => (
                    "右圆括号())".to_string(),
                    "右圆括号用于结束括号表达式或参数列表。可能的问题:\n    - 括号内缺少表达式，如: () 应该是 (x + y)\n    - 函数调用缺少参数，如: foo() 中的括号是空的，但可能需要参数"
                ),
                crate::lexer::Token::LBrace => (
                    "左花括号({)".to_string(),
                    "左花括号用于开始代码块。可能的问题:\n    - 在需要表达式的地方使用了代码块\n    - Lambda表达式缺少箭头，如: (x) { x + 1 } 应该是 (x) -> { x + 1 }"
                ),
                crate::lexer::Token::LBracket => (
                    "左方括号([)".to_string(),
                    "左方括号用于数组索引或注解。可能的问题:\n    - 数组索引前缺少数组对象\n    - 数组字面量需要在赋值右侧，如: int[] arr = {1, 2, 3} 是正确的"
                ),
                crate::lexer::Token::LParen => (
                    "左圆括号(()".to_string(),
                    "左圆括号用于开始括号表达式。可能的问题:\n    - 括号内缺少表达式\n    - Lambda表达式参数列表后缺少箭头，如: (x, y) x + y 应该是 (x, y) -> x + y"
                ),
                crate::lexer::Token::Comma => (
                    "逗号(,)".to_string(),
                    "逗号用于分隔参数或数组元素。可能的问题:\n    - 多余的逗号，如: foo(1, , 2) 应该是 foo(1, 2)\n    - 逗号前缺少表达式，如: int[] arr = {1, , 2} 应该是 int[] arr = {1, 2}"
                ),
                crate::lexer::Token::Dot => (
                    "点(.)".to_string(),
                    "点用于成员访问。可能的问题:\n    - 点前缺少对象，如: .field 应该是 obj.field\n    - 方法调用缺少对象，如: .method() 应该是 obj.method()"
                ),
                crate::lexer::Token::Colon => (
                    "冒号(:)".to_string(),
                    "冒号用于类型注解或三元运算符。可能的问题:\n    - 冒号前缺少变量名，如: : int 应该是 x: int\n    - 三元运算符缺少问号部分，如: x : y 应该是 cond ? x : y"
                ),
                // 访问修饰符
                crate::lexer::Token::Public => (
                    "关键字(public)".to_string(),
                    "public 用于声明公共成员。可能的问题:\n    - 在表达式位置使用了访问修饰符\n    - 类声明位置错误，如: x = public class 应该是 public class X { ... }"
                ),
                crate::lexer::Token::Private => (
                    "关键字(private)".to_string(),
                    "private 用于声明私有成员。可能的问题:\n    - 在表达式位置使用了访问修饰符\n    - 类成员声明位置错误"
                ),
                crate::lexer::Token::Protected => (
                    "关键字(protected)".to_string(),
                    "protected 用于声明受保护成员。可能的问题:\n    - 在表达式位置使用了访问修饰符\n    - 类成员声明位置错误"
                ),
                // 类型关键字
                crate::lexer::Token::Static => (
                    "关键字(static)".to_string(),
                    "static 用于声明静态成员。可能的问题:\n    - 在表达式位置使用了修饰符\n    - 静态成员声明位置错误，如: x = static int 应该是 static int x;"
                ),
                crate::lexer::Token::Final => (
                    "关键字(final)".to_string(),
                    "final 用于声明常量。可能的问题:\n    - 在表达式位置使用了修饰符\n    - 常量声明位置错误，如: x = final int 应该是 final int x = 10;"
                ),
                crate::lexer::Token::Abstract => (
                    "关键字(abstract)".to_string(),
                    "abstract 用于声明抽象类或方法。可能的问题:\n    - 在表达式位置使用了修饰符\n    - 抽象方法声明位置错误"
                ),
                // 类/接口声明
                crate::lexer::Token::Class => (
                    "关键字(class)".to_string(),
                    "class 用于声明类。可能的问题:\n    - 在表达式位置使用了类声明\n    - 类声明位置错误，如: x = class 应该是 class MyClass { ... }\n    - 类声明应在文件顶层或作为类型使用"
                ),
                crate::lexer::Token::Interface => (
                    "关键字(interface)".to_string(),
                    "interface 用于声明接口。可能的问题:\n    - 在表达式位置使用了接口声明\n    - 接口声明位置错误，如: x = interface 应该是 interface MyInterface { ... }"
                ),
                crate::lexer::Token::Extends => (
                    "关键字(extends)".to_string(),
                    "extends 用于类继承。可能的问题:\n    - 在表达式位置使用了继承关键字\n    - 继承声明位置错误，如: x = extends 应该是 class Child extends Parent { ... }"
                ),
                crate::lexer::Token::Implements => (
                    "关键字(implements)".to_string(),
                    "implements 用于实现接口。可能的问题:\n    - 在表达式位置使用了实现关键字\n    - 实现声明位置错误，如: x = implements 应该是 class MyClass implements Interface { ... }"
                ),
                // 类型
                crate::lexer::Token::Void => (
                    "关键字(void)".to_string(),
                    "void 表示无返回值。可能的问题:\n    - 在表达式位置使用了类型关键字\n    - void 只能用于方法返回类型，不能作为变量类型"
                ),
                crate::lexer::Token::Int | crate::lexer::Token::Long | 
                crate::lexer::Token::Float | crate::lexer::Token::Double |
                crate::lexer::Token::Bool | crate::lexer::Token::Char |
                crate::lexer::Token::String => (
                    format!("类型关键字({})", get_type_name(current_token)),
                    "类型关键字用于声明变量或方法返回类型。可能的问题:\n    - 在表达式位置使用了类型关键字\n    - 变量声明格式错误，如: int 应该是 int x; 或 int x = 10;\n    - 类型后缺少变量名"
                ),
                // 控制流
                crate::lexer::Token::If => (
                    "关键字(if)".to_string(),
                    "if 用于条件语句。可能的问题:\n    - 在表达式位置使用了 if 语句\n    - if 语句格式错误，如: x = if 应该是 if (cond) { ... }\n    - 三元运算符应使用 ? : 而不是 if-else"
                ),
                crate::lexer::Token::Else => (
                    "关键字(else)".to_string(),
                    "else 用于 if 语句的 else 分支。可能的问题:\n    - else 没有匹配的 if\n    - else 前缺少 if 语句，如: x = else 应该是 if (cond) { ... } else { ... }"
                ),
                crate::lexer::Token::For => (
                    "关键字(for)".to_string(),
                    "for 用于循环。可能的问题:\n    - 在表达式位置使用了 for 语句\n    - for 语句格式错误，如: x = for 应该是 for (init; cond; update) { ... }"
                ),
                crate::lexer::Token::While => (
                    "关键字(while)".to_string(),
                    "while 用于循环。可能的问题:\n    - 在表达式位置使用了 while 语句\n    - while 语句格式错误，如: x = while 应该是 while (cond) { ... }"
                ),
                crate::lexer::Token::Do => (
                    "关键字(do)".to_string(),
                    "do 用于 do-while 循环。可能的问题:\n    - 在表达式位置使用了 do 语句\n    - do-while 语句格式错误，如: x = do 应该是 do { ... } while (cond);"
                ),
                crate::lexer::Token::Switch => (
                    "关键字(switch)".to_string(),
                    "switch 用于多分支选择。可能的问题:\n    - 在表达式位置使用了 switch 语句\n    - switch 语句格式错误，如: x = switch 应该是 switch (expr) { case 1: ... }"
                ),
                crate::lexer::Token::Case => (
                    "关键字(case)".to_string(),
                    "case 用于 switch 语句的分支。可能的问题:\n    - case 不在 switch 语句内\n    - case 后缺少常量值，如: case: 应该是 case 1:"
                ),
                crate::lexer::Token::Default => (
                    "关键字(default)".to_string(),
                    "default 用于 switch 语句的默认分支。可能的问题:\n    - default 不在 switch 语句内\n    - default 后缺少冒号，如: default 应该是 default:"
                ),
                crate::lexer::Token::Break => (
                    "关键字(break)".to_string(),
                    "break 用于跳出循环或 switch。可能的问题:\n    - break 后多余的内容，如: break x 应该是 break;\n    - break 不在循环或 switch 内"
                ),
                crate::lexer::Token::Continue => (
                    "关键字(continue)".to_string(),
                    "continue 用于继续下一次循环。可能的问题:\n    - continue 后多余的内容，如: continue x 应该是 continue;\n    - continue 不在循环内"
                ),
                crate::lexer::Token::Return => (
                    "关键字(return)".to_string(),
                    "return 用于返回值。可能的问题:\n    - return 后缺少表达式或分号\n    - return 格式错误，如: return x y 应该是 return x;"
                ),
                // 特殊关键字
                crate::lexer::Token::New => (
                    "关键字(new)".to_string(),
                    "new 用于创建对象或数组。可能的问题:\n    - new 后缺少类型，如: new 应该是 new MyClass() 或 new int[10]\n    - 数组创建语法错误，如: new int 应该是 new int[10]"
                ),
                crate::lexer::Token::This => (
                    "关键字(this)".to_string(),
                    "this 指代当前对象。可能的问题:\n    - this 后错误的使用方式\n    - 静态上下文中使用了 this"
                ),
                crate::lexer::Token::Super => (
                    "关键字(super)".to_string(),
                    "super 指代父类。可能的问题:\n    - super 后错误的使用方式\n    - 静态上下文中使用了 super"
                ),
                crate::lexer::Token::Null => (
                    "关键字(null)".to_string(),
                    "null 表示空引用。这是一个有效的表达式。"
                ),
                crate::lexer::Token::True | crate::lexer::Token::False => (
                    "布尔字面量".to_string(),
                    "true/false 是有效的布尔表达式。"
                ),
                // 字面量
                crate::lexer::Token::Identifier(name) => (
                    format!("标识符('{}')", name),
                    "标识符可以作为表达式。可能的问题:\n    - 标识符前缺少对象，如: .method() 应该是 obj.method()\n    - 标识符未定义\n    - 标识符后缺少运算符，如: x y 应该是 x + y"
                ),
                crate::lexer::Token::IntegerLiteral(Some((val, _))) => (
                    format!("整数({})", val),
                    "整数字面量是有效的表达式。可能的问题:\n    - 数字后缺少运算符，如: 1 2 应该是 1 + 2\n    - 数字格式错误"
                ),
                crate::lexer::Token::FloatLiteral(Some((val, _))) => (
                    format!("浮点数({})", val),
                    "浮点数字面量是有效的表达式。可能的问题:\n    - 数字后缺少运算符\n    - 浮点数格式错误，如: 3.14.15 应该是 3.1415"
                ),
                crate::lexer::Token::StringLiteral(Some(s)) => (
                    format!("字符串(\"{}\")", s),
                    "字符串字面量是有效的表达式。可能的问题:\n    - 字符串未正确闭合，如: \"hello 应该是 \"hello\"\n    - 字符串后缺少运算符"
                ),
                crate::lexer::Token::CharLiteral(Some(c)) => (
                    format!("字符('{}')", c),
                    "字符字面量是有效的表达式。可能的问题:\n    - 字符未正确闭合，如: 'a 应该是 'a'\n    - 多字符字面量，如: 'ab' 应该是 \"ab\""
                ),
                // 其他
                _ => {
                    if parser.is_at_end() {
                        (
                            "文件结束(EOF)".to_string(),
                            "代码在表达式未完成时结束。可能的问题:\n    - 缺少表达式，如: x = \n    - 缺少右括号、右花括号或右方括号\n    - 语句缺少分号结束"
                        )
                    } else {
                        (
                            format!("{:?}", current_token),
                            "这是一个意外的标记。请检查语法是否正确。"
                        )
                    }
                }
            };
            Err(parser.error(&format!(
                "期望表达式，但遇到了 {}\n提示: {}",
                token_desc, suggestion
            )))
        }
    }
}

/// 解析 new 表达式（支持类创建和多维数组创建）
pub fn parse_new_expression(parser: &mut Parser, loc: crate::error::SourceLocation) -> cayResult<Expr> {
    // 首先尝试解析类型
    if is_type_token(parser) {
        // 解析基本类型或类名（不包含数组维度）
        let base_element_type = parse_base_type(parser)?;

        // 如果接下来是 '[' 则为数组创建: new Type[size] 或 new Type[size1][size2]...
        if parser.check(&crate::lexer::Token::LBracket) {
            let mut sizes = Vec::new();
            let mut has_empty_dimension = false;

            // 解析所有维度: [size1][size2]... 或 [size][]...
            while parser.match_token(&crate::lexer::Token::LBracket) {
                // 检查是否是空维度 [] (用于不规则数组，如 new int[5][])
                if parser.check(&crate::lexer::Token::RBracket) {
                    // 空维度，只有在不是第一个维度时才允许
                    if sizes.is_empty() {
                        return Err(parser.error(
                            "数组第一个维度必须指定大小\n\
                            提示: 不规则数组语法为 new Type[size][]，第一个维度必须有大小"
                        ));
                    }
                    has_empty_dimension = true;
                    parser.advance(); // 跳过 ']'
                    // 空维度用 null 表达式表示
                    let null_loc = parser.current_loc();
                    sizes.push(Expr::Literal(LiteralExpr { value: LiteralValue::Null, loc: null_loc }));
                } else {
                    // 正常维度，解析表达式
                    let size = parse_expression(parser)?;
                    parser.consume(&crate::lexer::Token::RBracket, "期望 ']'\n提示: 数组大小表达式应以 ']' 结束，例如: new int[10]")?;
                    sizes.push(size);
                }
            }

            // 构建多维元素类型: base_type[][]...
            let mut element_type = base_element_type;
            for _ in 1..sizes.len() {
                element_type = Type::Array(Box::new(element_type));
            }

            // 检查是否有 () 零初始化后缀
            let zero_init = parser.match_token(&crate::lexer::Token::LParen)
                && parser.match_token(&crate::lexer::Token::RParen);

            return Ok(Expr::ArrayCreation(ArrayCreationExpr {
                element_type,
                sizes,
                zero_init,
                loc,
            }));
        }

        // 如果接下来是 '(' 则为对象创建: new ClassName(...)
        if parser.match_token(&crate::lexer::Token::LParen) {
            // element_type should be Type::Object(name)
            match base_element_type {
                crate::types::Type::Object(name) => {
                    let args = parse_arguments(parser)?;
                    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after arguments")?;
                    return Ok(Expr::New(NewExpr { class_name: name, args, loc }));
                }
                _ => {
                    let type_name = format!("{:?}", base_element_type);
                    return Err(parser.error(&format!(
                        "类型 {} 不能使用 'new Type()' 构造\n\
                        提示: 只有类类型可以使用 'new' 构造，基本类型应使用数组语法: new {}[size]",
                        type_name, type_name
                    )));
                }
            }
        }

        // 否则既不是数组也不是对象构造，报错
        return Err(parser.error(
            "期望 '[' 或 '('\n\
            提示: new 表达式后应跟:\
            - 数组创建: new Type[size]\n\
            - 对象创建: new ClassName()"
        ));
    }

    // 普通类创建: new ClassName()
    let class_name = parser.consume_identifier("期望类名\n提示: new 后应跟类名，例如: new MyClass()")?;
    parser.consume(&crate::lexer::Token::LParen, "期望 '('\n提示: 类名后应跟 '(' 开始参数列表，例如: new MyClass()")?;
    let args = parse_arguments(parser)?;
    parser.consume(&crate::lexer::Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;
    Ok(Expr::New(NewExpr {
        class_name,
        args,
        loc,
    }))
}

/// 获取类型名称
fn get_type_name(token: &crate::lexer::Token) -> &'static str {
    match token {
        crate::lexer::Token::Int => "int",
        crate::lexer::Token::Long => "long",
        crate::lexer::Token::Float => "float",
        crate::lexer::Token::Double => "double",
        crate::lexer::Token::Bool => "bool",
        crate::lexer::Token::Char => "char",
        crate::lexer::Token::String => "String",
        crate::lexer::Token::Void => "void",
        _ => "unknown",
    }
}

/// 解析基本类型（不包含数组维度）
pub fn parse_base_type(parser: &mut Parser) -> cayResult<Type> {
    match parser.current_token() {
        crate::lexer::Token::Int => { parser.advance(); Ok(Type::Int32) }
        crate::lexer::Token::Long => { parser.advance(); Ok(Type::Int64) }
        crate::lexer::Token::Float => { parser.advance(); Ok(Type::Float32) }
        crate::lexer::Token::Double => { parser.advance(); Ok(Type::Float64) }
        crate::lexer::Token::Bool => { parser.advance(); Ok(Type::Bool) }
        crate::lexer::Token::String => { parser.advance(); Ok(Type::String) }
        crate::lexer::Token::Char => { parser.advance(); Ok(Type::Char) }
        crate::lexer::Token::Identifier(name) => {
            let mut name = name.clone();
            parser.advance();
            // 支持命名空间限定类型名: std::StringBuilder, graphics::Cube
            while parser.check(&crate::lexer::Token::DoubleColon) {
                parser.advance(); // 消费 ::
                let next = super::super::utils::consume_identifier(parser, "期望命名空间后的标识符\n提示: 限定类型名格式为 'ns::Type'")?;
                name = format!("{}::{}", name, next);
            }
            // 检查是否有泛型类型参数 <...>
            if parser.check(&crate::lexer::Token::Lt) {
                // 解析泛型类型参数: Optional<int>, Box<String>
                let type_args = crate::parser::classes::parse_generic_type_args(parser)?;
                // 将泛型类型格式化为完整的类型名称: "Optional<int>"
                let type_args_str = type_args.iter()
                    .map(|t| type_to_string(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                let generic_name = format!("{}<{}>", name, type_args_str);
                Ok(Type::Object(generic_name))
            } else {
                Ok(Type::Object(name))
            }
        }
        _ => {
            let current_token = super::super::utils::get_token_name(parser.current_token());
            Err(parser.error(&format!(
                "期望基本类型或类名，但遇到了 {}\n\
                提示: 基本类型包括 int, long, float, double, bool, char, String",
                current_token
            )))
        }
    }
}

/// 解析参数列表
fn parse_arguments(parser: &mut Parser) -> cayResult<Vec<Expr>> {
    let mut args = Vec::new();

    if !parser.check(&crate::lexer::Token::RParen) {
        loop {
            args.push(parse_expression(parser)?);
            if !parser.match_token(&crate::lexer::Token::Comma) {
                break;
            }
        }
    }

    Ok(args)
}

/// 将 Type 转换为字符串表示
/// 用于构建泛型类型名称，如 Optional<int>
fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Void => "void".to_string(),
        Type::Int32 => "int".to_string(),
        Type::Int64 => "long".to_string(),
        Type::Float32 => "float".to_string(),
        Type::Float64 => "double".to_string(),
        Type::Bool => "boolean".to_string(),
        Type::String => "String".to_string(),
        Type::Char => "char".to_string(),
        Type::Object(name) => name.clone(),
        Type::Array(elem_type) => format!("{}[]", type_to_string(elem_type)),
        Type::Pointer(elem_type) => format!("{}*", type_to_string(elem_type)),
        Type::Function(func_type) => {
            let params = func_type.params.iter()
                .map(type_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) -> {}", params, type_to_string(&func_type.return_type))
        }
        Type::Auto => "auto".to_string(),
        Type::GenericParam(name) => name.clone(),
        Type::Generic(name, args) => {
            let args_str = args.iter()
                .map(type_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", name, args_str)
        }
        // FFI 类型
        Type::CInt => "c_int".to_string(),
        Type::CUInt => "c_uint".to_string(),
        Type::CLong => "c_long".to_string(),
        Type::CULong => "c_ulong".to_string(),
        Type::CShort => "c_short".to_string(),
        Type::CUShort => "c_ushort".to_string(),
        Type::CChar => "c_char".to_string(),
        Type::CUChar => "c_uchar".to_string(),
        Type::CFloat => "c_float".to_string(),
        Type::CDouble => "c_double".to_string(),
        Type::SizeT => "size_t".to_string(),
        Type::SSizeT => "ssize_t".to_string(),
        Type::UIntPtr => "uintptr_t".to_string(),
        Type::IntPtr => "intptr_t".to_string(),
        Type::CVoid => "c_void".to_string(),
        Type::CBool => "c_bool".to_string(),
        Type::Struct(name) => format!("struct {}", name),
    }
}
