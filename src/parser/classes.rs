//! 类相关解析

use super::Parser;
use super::expressions::parse_expression;
use super::statements::{parse_block, parse_statement, promote_tail_to_return};
use super::types::{is_type_token, parse_type};
use crate::ast::*;
use crate::lexer::Token;
use crate::miette_diagnostic::CayResult;
use crate::miette_diagnostic::SourceLocation;
use crate::types::{InterfaceInfo, ParameterInfo, Type};

/// 解析类声明
pub fn parse_class(parser: &mut Parser) -> CayResult<ClassDecl> {
    let loc = parser.current_loc();

    // 解析所有修饰符（包括 @main 注解）
    let modifiers = parse_modifiers(parser)?;

    parser.consume(
        &Token::Class,
        "期望关键字 'class'\n提示: 类声明应以 'class' 开头，例如: class MyClass { ... }",
    )?;

    let name = parser
        .consume_identifier("期望类名\n提示: 在 'class' 后应跟类名，例如: class MyClass { ... }")?;

    // 解析泛型类型参数 <T, U, ...>
    let type_params = parse_generic_type_params(parser)?;

    // 支持 extends 关键字或 : 符号作为继承语法
    let parent = if parser.match_token(&Token::Extends) {
        Some(parser.consume_identifier(
            "期望父类名\n提示: 在 'extends' 后应跟父类名，例如: class Child extends Parent { ... }",
        )?)
    } else if parser.match_token(&Token::Colon) {
        // 保留 : 符号作为兼容语法
        Some(parser.consume_identifier(
            "期望父类名\n提示: 在 ':' 后应跟父类名，例如: class Child : Parent { ... }",
        )?)
    } else {
        None
    };

    // 解析实现的接口（支持泛型实参，如 implements Iterator<T>）
    let mut interfaces = Vec::new();
    if parser.match_token(&Token::Implements) {
        loop {
            let interface_type = parse_type(parser)?;
            interfaces.push(interface_type);
            if !parser.match_token(&Token::Comma) {
                break;
            }
        }
    }

    parser.consume(
        &Token::LBrace,
        "期望 '{'\n提示: 类声明后应跟类体，使用 '{' 开始，例如: class MyClass { ... }",
    )?;

    let mut members = Vec::new();
    while !parser.check(&Token::RBrace) && !parser.is_at_end() {
        members.push(parse_class_member(parser)?);
    }

    parser.consume(&Token::RBrace, "期望 '}'\n提示: 类体应以 '}' 结束")?;

    Ok(ClassDecl {
        name,
        modifiers,
        type_params,
        parent,
        interfaces,
        members,
        namespace_path: Vec::new(),
        loc,
    })
}

/// 解析接口声明
pub fn parse_interface(parser: &mut Parser) -> CayResult<InterfaceDecl> {
    let loc = parser.current_loc();

    // 解析修饰符
    let modifiers = parse_modifiers(parser)?;

    parser.consume(&Token::Interface, "期望关键字 'interface'\n提示: 接口声明应以 'interface' 开头，例如: interface MyInterface { ... }")?;

    let name = parser.consume_identifier(
        "期望接口名\n提示: 在 'interface' 后应跟接口名，例如: interface MyInterface { ... }",
    )?;
    // 解析泛型类型参数
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LBrace,
        "期望 '{'\n提示: 接口声明后应跟接口体，使用 '{' 开始，例如: interface MyInterface { ... }",
    )?;

    // 接口只能包含方法声明（没有方法体）
    let mut methods = Vec::new();
    while !parser.check(&Token::RBrace) && !parser.is_at_end() {
        methods.push(parse_interface_method(parser)?);
    }

    parser.consume(&Token::RBrace, "期望 '}'\n提示: 接口体应以 '}' 结束")?;

    Ok(InterfaceDecl {
        name,
        modifiers,
        type_params,
        methods,
        namespace_path: Vec::new(),
        loc,
    })
}

/// 解析接口方法（只有声明，没有实现）
fn parse_interface_method(parser: &mut Parser) -> CayResult<MethodDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;

    let return_type = if parser.check(&Token::Void) {
        parser.advance();
        Type::Void
    } else {
        parser.parse_type_or_fn_ptr()?
    };

    let name = parser.consume_identifier(
        "期望方法名\n提示: 在返回类型后应跟方法名，例如: int calculate() { ... }",
    )?;
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LParen,
        "期望 '('\n提示: 方法名后应跟 '(' 开始参数列表，例如: int calculate() { ... }",
    )?;
    let params = parse_parameters(parser)?;
    parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;

    // 接口方法必须以分号结束，没有方法体
    parser.consume(
        &Token::Semicolon,
        "期望 ';'\n提示: 接口方法声明应以 ';' 结束，例如: int calculate();",
    )?;

    Ok(MethodDecl {
        name,
        type_params,
        modifiers,
        return_type,
        params,
        body: None, // 接口方法没有方法体
        loc,
    })
}

/// 解析类成员（字段、方法、构造函数、析构函数或初始化块）
pub fn parse_class_member(parser: &mut Parser) -> CayResult<ClassMember> {
    // 向前看判断成员类型
    let checkpoint = parser.pos;
    let modifiers = parse_modifiers(parser)?;

    //eprintln!("[DEBUG] parse_class_member after modifiers, current token: {:?}", parser.current_token());

    // 检查是否是静态初始化块 static { ... }
    if modifiers.contains(&Modifier::Static) && parser.check(&Token::LBrace) {
        parser.pos = checkpoint;
        return Ok(ClassMember::StaticInitializer(parse_static_initializer(
            parser,
        )?));
    }

    // 检查是否是初始化块 { ... }
    if parser.check(&Token::LBrace) {
        parser.pos = checkpoint;
        return Ok(ClassMember::InstanceInitializer(
            parse_instance_initializer(parser)?,
        ));
    }

    // 检查是否是析构函数 ~ClassName() { ... }
    if parser.check(&Token::Tilde) {
        parser.pos = checkpoint;
        return Ok(ClassMember::Destructor(parse_destructor(parser)?));
    }

    // 如果是void，一定是方法返回类型
    if parser.check(&Token::Void) {
        parser.pos = checkpoint;
        return Ok(ClassMember::Method(parse_method(parser)?));
    }

    // 检查是否是构造函数：类名(...)
    // 构造函数的特征是：标识符后直接跟'('，且不是类型关键字（void, int等）
    if matches!(parser.current_token(), Token::Identifier(_)) {
        // 向前看：检查下一个token是否是 '('
        let current_pos = parser.pos;
        parser.advance(); // 跳过标识符

        if parser.check(&Token::LParen) {
            // 是构造函数 - 回溯到checkpoint并解析
            parser.pos = checkpoint;

            // 直接解析构造函数
            let loc = parser.current_loc();
            let ctor_modifiers = parse_modifiers(parser)?;
            let _ctor_name = parser.consume_identifier("Expected constructor name")?;

            parser.consume(&Token::LParen, "Expected '(' after constructor name")?;
            let ctor_params = parse_parameters(parser)?;
            parser.consume(&Token::RParen, "Expected ')' after constructor parameters")?;

            // 解析构造链调用 this() 或 super()
            let ctor_call_result = parse_constructor_call(parser)?;
            let constructor_call = ctor_call_result.call;

            // 解析构造函数体
            // native/abstract 构造函数无实现体，以 ';' 结束
            let ctor_body = if ctor_modifiers.contains(&Modifier::Native)
                || ctor_modifiers.contains(&Modifier::Abstract)
            {
                parser.consume(
                    &Token::Semicolon,
                    "期望 ';'\n提示: native/abstract 构造函数声明应以 ';' 结束，例如: native MyClass();",
                )?;
                Block {
                    statements: Vec::new(),
                    tail_expr: None,
                    loc: parser.current_loc(),
                }
            } else if ctor_call_result.consumed_lbrace {
                // 已经消耗了 {，直接解析语句直到 }
                let mut statements = Vec::new();
                while !parser.check(&Token::RBrace) && !parser.is_at_end() {
                    statements.push(parse_statement(parser)?);
                }
                parser.consume(&Token::RBrace, "期望 '}'\n提示: 构造函数体应以 '}' 结束")?;
                Block {
                    statements,
                    tail_expr: None,
                    loc: parser.current_loc(),
                }
            } else {
                parse_block(parser)?
            };

            return Ok(ClassMember::Constructor(ConstructorDecl {
                modifiers: ctor_modifiers,
                params: ctor_params,
                body: ctor_body,
                constructor_call,
                loc,
            }));
        } else {
            // 不是构造函数，回退位置
            parser.pos = current_pos;
        }
    }

    // 检查是否是 fn 关键字开头的方法（类型后置式/自动推断）: fn name(params) [ret] { ... }
    if parser.check(&Token::Fn) {
        let pos_after_fn = parser.pos + 1;
        if pos_after_fn < parser.tokens.len()
            && matches!(&parser.tokens[pos_after_fn].token, Token::Identifier(_))
        {
            parser.pos = checkpoint;
            return Ok(ClassMember::Method(parse_fn_method(parser)?));
        }
    }

    // 检查是否是函数指针类型开头的方法: fn(...) -> ReturnType name(...)
    if parser.check(&Token::Fn) {
        parser.pos = checkpoint;
        return Ok(ClassMember::Method(parse_method(parser)?));
    }

    // 如果是类型关键字，可能是字段或方法
    if is_type_token(parser) {
        // 读取类型
        let member_type = parse_type(parser)?;
        let member_name = parser.consume_identifier("期望成员名\n提示: 类型后应跟字段名或方法名，例如: int count; 或 int calculate() { ... }")?;

        if parser.check(&Token::LParen) {
            // 是方法
            parser.pos = checkpoint;
            Ok(ClassMember::Method(parse_method(parser)?))
        } else {
            // 是字段
            parser.pos = checkpoint;
            Ok(ClassMember::Field(parse_field(parser)?))
        }
    } else {
        let current_token = parser.current_token();
        let (token_desc, suggestion) = match current_token {
            // 分隔符
            crate::lexer::Token::Semicolon => (
                "分号(;)".to_string(),
                "类成员声明不能是空语句。可能的问题:\n    - 多余的逗号或分号\n    - 缺少成员声明".to_string()
            ),
            crate::lexer::Token::Comma => (
                "逗号(,)".to_string(),
                "逗号不能开始成员声明。可能的问题:\n    - 成员声明之间多余的逗号\n    - 字段声明格式错误".to_string()
            ),
            crate::lexer::Token::LParen => (
                "左圆括号(()".to_string(),
                "括号不能开始成员声明。可能的问题:\n    - 缺少返回类型，如: (x) 应该是 int calc(x)\n    - 类型声明位置错误".to_string()
            ),
            crate::lexer::Token::RParen => (
                "右圆括号())".to_string(),
                "括号不能开始成员声明。可能的问题:\n    - 前面的声明缺少左括号\n    - 多余的右括号".to_string()
            ),
            crate::lexer::Token::LBrace => (
                "左花括号({)".to_string(),
                "代码块开始不能作为成员声明。可能的问题:\n    - 缺少类成员声明\n    - 方法体缺少签名".to_string()
            ),
            crate::lexer::Token::RBrace => (
                "右花括号(})".to_string(),
                "类声明提前结束。可能的问题:\n    - 类体为空\n    - 前面的声明语法错误".to_string()
            ),
            // 关键字
            crate::lexer::Token::Class => (
                "关键字(class)".to_string(),
                "类声明不能在类内部。可能的问题:\n    - 嵌套类不支持\n    - 类声明位置错误".to_string()
            ),
            crate::lexer::Token::Interface => (
                "关键字(interface)".to_string(),
                "接口声明不能在类内部。可能的问题:\n    - 接口声明位置错误".to_string()
            ),
            crate::lexer::Token::If | crate::lexer::Token::Else |
            crate::lexer::Token::While | crate::lexer::Token::For |
            crate::lexer::Token::Do | crate::lexer::Token::Switch |
            crate::lexer::Token::Case | crate::lexer::Token::Default |
            crate::lexer::Token::Break | crate::lexer::Token::Continue |
            crate::lexer::Token::Return => {
                let kw = format!("{:?}", current_token).to_lowercase();
                (
                    format!("关键字({})", kw),
                    format!("{} 是控制流关键字，不能作为类成员。可能的问题:\n    - 控制流语句只能在方法体内使用\n    - 缺少方法声明", kw)
                )
            }
            // 修饰符（但后面没有有效成员）
            crate::lexer::Token::Public | crate::lexer::Token::Private |
            crate::lexer::Token::Protected | crate::lexer::Token::Static |
            crate::lexer::Token::Final | crate::lexer::Token::Abstract |
            crate::lexer::Token::Interop => {
                let kw = format!("{:?}", current_token).to_lowercase();
                (
                    format!("关键字({})", kw),
                    format!("修饰符 '{}' 后缺少有效的成员声明。可能的问题:\n    - 修饰符后缺少类型，如: public x; 应该是 public int x;\n    - 成员声明语法错误", kw)
                )
            }
            // 字面量
            crate::lexer::Token::IntegerLiteral(Some((val, _))) => (
                format!("整数({})", val),
                "整数字面量不能作为类成员。可能的问题:\n    - 缺少字段类型，如: 10; 应该是 int x = 10;\n    - 语句位置错误".to_string()
            ),
            crate::lexer::Token::FloatLiteral(Some((val, _))) => (
                format!("浮点数({})", val),
                "浮点数字面量不能作为类成员。可能的问题:\n    - 缺少字段类型\n    - 语句位置错误".to_string()
            ),
            crate::lexer::Token::StringLiteral(Some(s)) => (
                format!("字符串(\"{}\")", s),
                "字符串字面量不能作为类成员。可能的问题:\n    - 缺少字段类型，如: \"hello\"; 应该是 String s = \"hello\";\n    - 语句位置错误".to_string()
            ),
            // 标识符（可能是未定义的类型）
            crate::lexer::Token::Identifier(name) => {
                let name_owned = name.clone();
                (
                    format!("标识符('{}')", name_owned),
                    format!("'{}' 不是已知的类型。可能的问题:\n    - 类名拼写错误\n    - 需要先定义类 '{}' 再使用\n    - 缺少 import\n    - 如果是方法调用，应在方法体内使用", name_owned, name_owned)
                )
            }
            // 其他
            _ => {
                let token_name = super::utils::get_token_name(current_token);
                (
                    token_name.clone(),
                    format!("{} 不能作为类成员开始。类成员可以是:\n    - 字段: int count;\n    - 方法: int calculate() {{ ... }}\n    - 构造函数: ClassName() {{ ... }}\n    - 析构函数: ~ClassName() {{ ... }}", token_name)
                )
            }
        };
        Err(parser.error(&format!(
            "期望字段、方法、构造函数或析构函数声明，但遇到了 {}\n提示: {}",
            token_desc, suggestion
        )))
    }
}

/// 解析字段声明
pub fn parse_field(parser: &mut Parser) -> CayResult<FieldDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;
    let field_type = parse_type(parser)?;
    let name = parser.consume_identifier("期望字段名\n提示: 类型后应跟字段名，例如: int count;")?;

    let initializer = if parser.match_token(&Token::Assign) {
        Some(parse_expression(parser)?)
    } else {
        None
    };

    parser.consume(
        &Token::Semicolon,
        "期望 ';'\n提示: 字段声明应以 ';' 结束，例如: int count;",
    )?;

    Ok(FieldDecl {
        name,
        field_type,
        modifiers,
        initializer,
        loc,
    })
}

/// 解析方法声明
pub fn parse_method(parser: &mut Parser) -> CayResult<MethodDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;

    let return_type = if parser.check(&Token::Void) {
        parser.advance();
        Type::Void
    } else {
        parser.parse_type_or_fn_ptr()?
    };

    let name = parser.consume_identifier(
        "期望方法名\n提示: 返回类型后应跟方法名，例如: int calculate() { ... }",
    )?;
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LParen,
        "期望 '('\n提示: 方法名后应跟 '(' 开始参数列表，例如: int calculate() { ... }",
    )?;
    let params = parse_parameters(parser)?;
    parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;

    // 检查是否是native方法或abstract方法（这两种都可以没有方法体）
    let is_native = modifiers.contains(&Modifier::Native);
    let is_abstract = modifiers.contains(&Modifier::Abstract);

    let body = if is_native || is_abstract {
        parser.consume(
            &Token::Semicolon,
            "期望 ';'\n提示: native/abstract 方法声明应以 ';' 结束，例如: native int foo();",
        )?;
        None
    } else {
        let mut body = parse_block(parser)?;
        // 6.2.x: 函数体尾表达式提升为隐式 return
        promote_tail_to_return(&mut body);
        Some(body)
    };

    Ok(MethodDecl {
        name,
        type_params,
        modifiers,
        return_type,
        params,
        body,
        loc,
    })
}

/// 解析 fn 关键字开头的方法声明（类型后置式/自动推断）
/// 格式: [modifiers] fn name(params) [return_type] { body }
/// 或: [modifiers] fn name(params) [return_type] ;
pub fn parse_fn_method(parser: &mut Parser) -> CayResult<MethodDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;

    parser.consume(
        &Token::Fn,
        "期望 'fn'\n提示: 方法定义应以 fn 开头，例如: fn calculate(int a, int b) { ... }",
    )?;

    let name = parser.consume_identifier(
        "期望方法名\n提示: fn 后应跟方法名，例如: fn calculate(int a, int b) { ... }",
    )?;
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LParen,
        "期望 '('\n提示: 方法名后应跟 '(' 开始参数列表，例如: calculate(int a, int b)",
    )?;
    let params = parse_parameters(parser)?;
    parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;

    // 解析可选的返回类型（类型后置式）
    let return_type = if parser.check(&Token::Arrow) {
        parser.advance();
        parser.parse_type_or_fn_ptr()?
    } else {
        crate::types::Type::Auto
    };

    // 检查是否是native方法或abstract方法（这两种都可以没有方法体）
    let is_native = modifiers.contains(&Modifier::Native);
    let is_abstract = modifiers.contains(&Modifier::Abstract);

    let body = if is_native || is_abstract {
        parser.consume(
            &Token::Semicolon,
            "期望 ';'\n提示: native/abstract 方法声明应以 ';' 结束，例如: native fn foo();",
        )?;
        None
    } else {
        let mut body = parse_block(parser)?;
        // 6.2.x: 函数体尾表达式提升为隐式 return
        promote_tail_to_return(&mut body);
        Some(body)
    };

    Ok(MethodDecl {
        name,
        type_params,
        modifiers,
        return_type,
        params,
        body,
        loc,
    })
}

/// 解析构造函数声明
/// 格式: [modifiers] ClassName([params]) [throws ...] { body }
/// 或: [modifiers] ClassName([params]) : this(args) { body }
/// 或: [modifiers] ClassName([params]) : super(args) { body }
pub fn parse_constructor(parser: &mut Parser) -> CayResult<ConstructorDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;

    // 构造函数名（必须与类名相同）
    let _name = parser.consume_identifier(
        "期望构造函数名\n提示: 构造函数名应与类名相同，例如: class MyClass { MyClass() { ... } }",
    )?;

    parser.consume(
        &Token::LParen,
        "期望 '('\n提示: 构造函数名后应跟 '(' 开始参数列表，例如: MyClass() { ... }",
    )?;
    let params = parse_parameters(parser)?;
    parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;

    // 解析构造链调用 this() 或 super()
    let ctor_call_result = parse_constructor_call(parser)?;
    let constructor_call = ctor_call_result.call;

    // 解析构造函数体
    // 如果 Java 风格的构造链调用已经消耗了 {，则不需要再解析 {
    let body = if ctor_call_result.consumed_lbrace {
        // 已经消耗了 {，直接解析语句直到 }
        let mut statements = Vec::new();
        while !parser.check(&Token::RBrace) && !parser.is_at_end() {
            statements.push(parse_statement(parser)?);
        }
        parser.consume(&Token::RBrace, "Expected '}' after constructor body")?;
        Block {
            statements,
            tail_expr: None,
            loc: parser.current_loc(),
        }
    } else {
        parse_block(parser)?
    };

    Ok(ConstructorDecl {
        modifiers,
        params,
        body,
        constructor_call,
        loc,
    })
}

/// 构造链调用解析结果
#[derive(Debug)]
struct ConstructorCallResult {
    pub call: Option<ConstructorCall>,
    pub consumed_lbrace: bool, // 是否消耗了左大括号
}

/// 解析构造链调用 this() 或 super()
/// 支持两种风格：
/// - C++风格: : this(args) 或 : super(args)（在构造函数参数列表后）
/// - Java风格: this(args) 或 super(args)（作为构造函数体的第一条语句）
fn parse_constructor_call(parser: &mut Parser) -> CayResult<ConstructorCallResult> {
    // 检查是否有冒号（C++风格）
    if parser.match_token(&Token::Colon) {
        // C++风格: : this(args) 或 : super(args)
        if parser.match_token(&Token::This) {
            parser.consume(
                &Token::LParen,
                "期望 '('\n提示: 'this' 后应跟 '(' 开始参数列表，例如: : this(args)",
            )?;
            let args = parse_constructor_call_args(parser)?;
            parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;
            return Ok(ConstructorCallResult {
                call: Some(ConstructorCall::This(args)),
                consumed_lbrace: false,
            });
        } else if parser.match_token(&Token::Super) {
            parser.consume(
                &Token::LParen,
                "期望 '('\n提示: 'super' 后应跟 '(' 开始参数列表，例如: : super(args)",
            )?;
            let args = parse_constructor_call_args(parser)?;
            parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;
            return Ok(ConstructorCallResult {
                call: Some(ConstructorCall::Super(args)),
                consumed_lbrace: false,
            });
        } else {
            let current_token = parser.current_token();
            let token_desc = super::utils::get_token_name(current_token);
            return Err(parser.error(&format!(
                "期望 'this' 或 'super'，但遇到了 {}\n\
                提示: 构造函数链调用应使用 : this(args) 或 : super(args) 语法",
                token_desc
            )));
        }
    }

    // Java风格: 检查是否是 this(args) 或 super(args) 作为第一条语句
    // 向前看：{ this( 或 { super(
    if parser.check(&Token::LBrace) {
        // 保存当前位置
        let checkpoint = parser.pos;
        parser.advance(); // 跳过 {

        // 检查是否是 this(
        if parser.match_token(&Token::This) {
            if parser.check(&Token::LParen) {
                parser.advance(); // 跳过 (
                let args = parse_constructor_call_args(parser)?;
                parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;
                parser.consume(
                    &Token::Semicolon,
                    "期望 ';'\n提示: this() 调用应以 ';' 结束",
                )?;
                return Ok(ConstructorCallResult {
                    call: Some(ConstructorCall::This(args)),
                    consumed_lbrace: true,
                });
            } else {
                // 不是 this(...)，回退
                parser.pos = checkpoint;
            }
        } else if parser.match_token(&Token::Super) {
            if parser.check(&Token::LParen) {
                parser.advance(); // 跳过 (
                let args = parse_constructor_call_args(parser)?;
                parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;
                parser.consume(
                    &Token::Semicolon,
                    "期望 ';'\n提示: super() 调用应以 ';' 结束",
                )?;
                return Ok(ConstructorCallResult {
                    call: Some(ConstructorCall::Super(args)),
                    consumed_lbrace: true,
                });
            } else {
                // 不是 super(...)，回退
                parser.pos = checkpoint;
            }
        } else {
            // 不是 this 或 super，回退
            parser.pos = checkpoint;
        }
    }

    Ok(ConstructorCallResult {
        call: None,
        consumed_lbrace: false,
    })
}

/// 解析构造函数调用参数
fn parse_constructor_call_args(parser: &mut Parser) -> CayResult<Vec<Expr>> {
    let mut args = Vec::new();

    if !parser.check(&Token::RParen) {
        loop {
            args.push(parse_expression(parser)?);
            if !parser.match_token(&Token::Comma) {
                break;
            }
        }
    }

    Ok(args)
}

/// 解析析构函数声明
/// 格式: ~ClassName() { body }
pub fn parse_destructor(parser: &mut Parser) -> CayResult<DestructorDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;

    // 消耗 ~
    parser.consume(
        &Token::Tilde,
        "期望 '~'\n提示: 析构函数以 '~' 开头，例如: ~MyClass() { ... }",
    )?;

    // 析构函数名（必须与类名相同）
    let _name = parser.consume_identifier(
        "期望析构函数名\n提示: 析构函数名应与类名相同，例如: ~MyClass() { ... }",
    )?;

    parser.consume(
        &Token::LParen,
        "期望 '('\n提示: 析构函数名后应跟 '()'，例如: ~MyClass()",
    )?;
    parser.consume(&Token::RParen, "期望 ')'\n提示: 析构函数不接受参数")?;

    // 解析析构函数体
    let body = parse_block(parser)?;

    Ok(DestructorDecl {
        modifiers,
        body,
        loc,
    })
}

/// 解析实例初始化块
/// 格式: { statements }
pub fn parse_instance_initializer(parser: &mut Parser) -> CayResult<Block> {
    parse_block(parser)
}

/// 解析静态初始化块
/// 格式: static { statements }
pub fn parse_static_initializer(parser: &mut Parser) -> CayResult<Block> {
    let _modifiers = parse_modifiers(parser)?; // 消耗 static
    parse_block(parser)
}

/// 解析修饰符列表（包括注解）
pub fn parse_modifiers(parser: &mut Parser) -> CayResult<Vec<Modifier>> {
    let mut modifiers = Vec::new();

    loop {
        match parser.current_token() {
            Token::Public => {
                modifiers.push(Modifier::Public);
                parser.advance();
            }
            Token::Private => {
                modifiers.push(Modifier::Private);
                parser.advance();
            }
            Token::Protected => {
                modifiers.push(Modifier::Protected);
                parser.advance();
            }
            Token::Static => {
                modifiers.push(Modifier::Static);
                parser.advance();
            }
            Token::Final => {
                modifiers.push(Modifier::Final);
                parser.advance();
            }
            Token::Abstract => {
                modifiers.push(Modifier::Abstract);
                parser.advance();
            }
            Token::Native => {
                modifiers.push(Modifier::Native);
                parser.advance();
            }
            Token::Interop => {
                modifiers.push(Modifier::Interop);
                parser.advance();
            }
            Token::AtOverride => {
                modifiers.push(Modifier::Override);
                parser.advance();
            }
            Token::AtMain => {
                modifiers.push(Modifier::Main);
                parser.advance();
            }
            Token::AtTest => {
                modifiers.push(Modifier::Test);
                parser.advance();
            }
            Token::AtFreeFunction => {
                modifiers.push(Modifier::FreeFunction);
                parser.advance();
            }
            Token::AtStackOnly => {
                modifiers.push(Modifier::StackOnly);
                parser.advance();
            }
            _ => break,
        }
    }

    Ok(modifiers)
}

/// 解析参数列表（支持可变参数）
pub fn parse_parameters(parser: &mut Parser) -> CayResult<Vec<ParameterInfo>> {
    //eprintln!("[DEBUG] parse_parameters called");
    let mut params = Vec::new();

    if !parser.check(&Token::RParen) {
        loop {
            // 检查是否是裸可变参数 ...（C 风格 extern 函数声明，如 int printf(const char* fmt, ...);）
            if parser.check(&Token::DotDotDot) {
                parser.advance(); // 消费 ...
                // 为可变参数创建一个特殊参数名
                params.push(ParameterInfo::new_varargs("...".to_string(), Type::CVoid));
                // 可变参数必须是最后一个参数
                if parser.check(&Token::Comma) {
                    return Err(parser.error(
                        "可变参数必须是最后一个参数\n提示: 可变参数(...)必须放在参数列表的最后",
                    ));
                }
                break;
            }

            // 6.2.x: 参数后置类型 name: Type（预读：标识符后跟冒号）
            // 与传统 Type name 形式并存，可混用: fn add(a: i32, b: i32) -> i32
            if let Token::Identifier(param_name) = parser.current_token().clone() {
                let saved_pos = parser.pos;
                parser.advance(); // 消费标识符
                if parser.check(&Token::Colon) {
                    parser.advance(); // 消费冒号
                    let param_type = parser.parse_type_or_fn_ptr()?;
                    if parser.match_token(&Token::DotDotDot) {
                        params.push(ParameterInfo::new_varargs(param_name, param_type));
                    } else {
                        params.push(ParameterInfo::new(param_name, param_type));
                    }
                    if !parser.match_token(&Token::Comma) {
                        break;
                    }
                    continue;
                }
                // 不是后置类型格式，回退到类型前置解析
                parser.pos = saved_pos;
            }

            // 检查是否是可变参数类型（type...）
            //eprintln!("[DEBUG] About to call parse_type_or_fn_ptr");
            let param_type = parser.parse_type_or_fn_ptr()?;
            //eprintln!("[DEBUG] parse_type_or_fn_ptr returned: {:?}", param_type);

            // 检查是否有 ... 标记
            let is_varargs = parser.match_token(&Token::DotDotDot);

            if is_varargs {
                // type... 形式的可变参数，需要一个名称
                let name = parser
                    .consume_identifier("期望参数名\n提示: 可变参数需要名称，例如: int... args")?;
                params.push(ParameterInfo::new_varargs(name, param_type));
                // 可变参数之后可以有更多参数（通过命名参数指定）
                if parser.match_token(&Token::Comma) {
                    continue;
                }
                break;
            } else {
                let name =
                    parser.consume_identifier("期望参数名\n提示: 参数需要名称，例如: int count")?;
                params.push(ParameterInfo::new(name, param_type));
            }

            if !parser.match_token(&Token::Comma) {
                break;
            }
        }
    }

    Ok(params)
}

/// 解析 struct 声明
/// struct Point { int x; int y; }
pub fn parse_struct(parser: &mut Parser) -> CayResult<StructDecl> {
    let loc = parser.current_loc();

    // 解析所有修饰符
    let modifiers = parse_modifiers(parser)?;

    parser.consume(&Token::Struct, "期望关键字 'struct'\n提示: struct 声明应以 'struct' 开头，例如: struct Point { int x; int y; }")?;

    let name = parser.consume_identifier(
        "期望 struct 名\n提示: 在 'struct' 后应跟 struct 名，例如: struct Point { ... }",
    )?;

    // 解析泛型类型参数 <T, U, ...>
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LBrace,
        "期望 '{'\n提示: struct 声明后应跟结构体，使用 '{' 开始，例如: struct Point { ... }",
    )?;

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut constructors = Vec::new();
    while !parser.check(&Token::RBrace) && !parser.is_at_end() {
        // struct 成员可以是字段、方法或构造函数
        let checkpoint = parser.pos;
        let member_modifiers = parse_modifiers(parser)?;

        // 检查是否是构造函数：struct 名后跟 '('
        let is_constructor = if let Token::Identifier(ref id) = parser.current_token().clone() {
            if id.as_str() == name.as_str() {
                let saved_pos = parser.pos;
                parser.advance();
                let looks_like_ctor = parser.check(&Token::LParen);
                parser.pos = saved_pos;
                looks_like_ctor
            } else {
                false
            }
        } else {
            false
        };

        if is_constructor {
            // 回退到 modifiers 之前，复用类构造函数解析逻辑
            parser.pos = checkpoint;
            constructors.push(parse_constructor(parser)?);
            continue;
        }

        // 检查是否是方法（返回类型 + 方法名 + 括号）
        let is_method = if parser.check(&Token::Void) || super::types::is_type_token(parser) {
            // 可能是方法 - 检查后面是否有方法名和括号
            let saved_pos = parser.pos;
            // 尝试跳过返回类型
            parser.advance(); // 消费返回类型 token
            let looks_like_method = if parser.is_at_end() {
                false
            } else if let Token::Identifier(_) = parser.current_token().clone() {
                parser.advance(); // 消费标识符
                parser.check(&Token::LParen)
            } else {
                false
            };
            parser.pos = saved_pos; // 回退
            looks_like_method
        } else {
            // 检查是否是构造函数（struct 名后跟括号）
            if let Token::Identifier(ref id) = parser.current_token().clone() {
                if id.as_str() == name.as_str() {
                    let saved_pos = parser.pos;
                    parser.advance();
                    if parser.check(&Token::LParen) {
                        parser.pos = saved_pos;
                        true
                    } else {
                        parser.pos = saved_pos;
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        };

        if is_method {
            // 解析为方法
            let method = parse_struct_method(parser, &member_modifiers)?;
            methods.push(method);
        } else {
            // 解析为字段
            let field = parse_struct_field(parser, &member_modifiers)?;
            fields.push(field);
        }
    }

    parser.consume(&Token::RBrace, "期望 '}'\n提示: struct 体应以 '}' 结束")?;

    Ok(StructDecl {
        name,
        modifiers,
        type_params,
        fields,
        methods,
        constructors,
        namespace_path: Vec::new(),
        loc,
    })
}

/// 解析 struct 字段 - int x;
fn parse_struct_field(parser: &mut Parser, modifiers: &[Modifier]) -> CayResult<FieldDecl> {
    let loc = parser.current_loc();
    let field_type = super::types::parse_type(parser)?;
    let name = parser.consume_identifier("期望字段名\n提示: 在类型后应跟字段名，例如: int x;")?;

    // 可选初始值
    let initializer = if parser.match_token(&Token::Assign) {
        Some(super::expressions::parse_expression(parser)?)
    } else {
        None
    };

    parser.consume(&Token::Semicolon, "期望 ';'\n提示: 字段声明应以 ';' 结束")?;

    Ok(FieldDecl {
        name,
        field_type,
        modifiers: modifiers.to_vec(),
        initializer,
        loc,
    })
}

/// 解析 struct 方法
fn parse_struct_method(parser: &mut Parser, modifiers: &[Modifier]) -> CayResult<MethodDecl> {
    parse_method_after_modifiers(parser, modifiers)
}

/// 在已有 modifiers 后继续解析方法
fn parse_method_after_modifiers(
    parser: &mut Parser,
    modifiers: &[Modifier],
) -> CayResult<MethodDecl> {
    let loc = parser.current_loc();

    // 检查是否是构造函数（struct 名后跟括号）
    let saved_pos = parser.pos;

    let return_type = if parser.check(&Token::Void) {
        parser.advance();
        Type::Void
    } else {
        parser.parse_type_or_fn_ptr()?
    };

    let name = parser.consume_identifier(
        "期望方法名\n提示: 在返回类型后应跟方法名，例如: int calculate() { ... }",
    )?;
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LParen,
        "期望 '('\n提示: 方法名后应跟 '(' 开始参数列表",
    )?;
    let params = parse_parameters(parser)?;
    parser.consume(&Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;

    // 检查是否有方法体
    let body = if parser.check(&Token::LBrace) {
        Some(super::statements::parse_block(parser)?)
    } else if parser.check(&Token::Semicolon) {
        // 抽象方法或声明
        parser.advance();
        None
    } else {
        return Err(parser.error("期望 '{' 或 ';'\n提示: 方法后应跟方法体或分号"));
    };

    Ok(MethodDecl {
        name,
        type_params,
        modifiers: modifiers.to_vec(),
        return_type,
        params,
        body,
        loc,
    })
}

/// 解析 enum 声明 - tagged union / ADT
/// enum Option<T> { Some(T), None }
pub fn parse_enum(parser: &mut Parser) -> CayResult<EnumDecl> {
    let loc = parser.current_loc();

    // 解析所有修饰符
    let modifiers = parse_modifiers(parser)?;

    parser.consume(&Token::Enum, "期望关键字 'enum'\n提示: enum 声明应以 'enum' 开头，例如: enum Option<T> { Some(T), None }")?;

    let name = parser.consume_identifier(
        "期望 enum 名\n提示: 在 'enum' 后应跟 enum 名，例如: enum Option<T> { ... }",
    )?;

    // 解析泛型类型参数 <T, U, ...>
    let type_params = parse_generic_type_params(parser)?;

    parser.consume(
        &Token::LBrace,
        "期望 '{'\n提示: enum 声明后应跟枚举体，使用 '{' 开始，例如: enum Option<T> { ... }",
    )?;

    let mut variants = Vec::new();
    while !parser.check(&Token::RBrace) && !parser.is_at_end() {
        let variant_loc = parser.current_loc();
        let variant_name = parser.consume_identifier(
            "期望 variant 名\n提示: enum variant 应为标识符，例如: Some 或 None",
        )?;

        // 检查是否携带 payload: Variant(Type)
        let payload_type = if parser.check(&Token::LParen) {
            parser.advance(); // 消费 (
            let ptype = super::types::parse_type(parser)?;
            parser.consume(
                &Token::RParen,
                "期望 ')'\n提示: variant payload 参数列表应以 ')' 结束",
            )?;
            Some(ptype)
        } else {
            None
        };

        variants.push(EnumVariant {
            name: variant_name,
            payload_type,
            loc: variant_loc,
        });

        // 检查逗号或结束
        if !parser.match_token(&Token::Comma) {
            break;
        }
    }

    parser.consume(&Token::RBrace, "期望 '}'\n提示: enum 体应以 '}' 结束")?;

    Ok(EnumDecl {
        name,
        modifiers,
        type_params,
        variants,
        namespace_path: Vec::new(),
        loc,
    })
}

/// 解析显式特化类声明: specialize class Box<int> { ... }
pub fn parse_specialize_class(parser: &mut Parser) -> CayResult<SpecializeClassDecl> {
    let loc = parser.current_loc();

    parser.consume(
        &Token::Specialize,
        "期望关键字 'specialize'\n提示: 显式特化声明应以 'specialize' 开头",
    )?;
    parser.consume(
        &Token::Class,
        "期望关键字 'class'\n提示: specialize 后应跟 'class'，例如: specialize class Box<int> { ... }",
    )?;

    let base_name =
        parser.consume_identifier("期望类名\n提示: 在 'specialize class' 后应跟类名")?;

    // 解析特化类型参数 <int> 或 <int, String>
    let type_args = parse_generic_type_args(parser)?;

    parser.consume(&Token::LBrace, "期望 '{'\n提示: 显式特化声明后应跟类体")?;

    let mut members = Vec::new();
    while !parser.check(&Token::RBrace) && !parser.is_at_end() {
        members.push(parse_class_member(parser)?);
    }

    parser.consume(&Token::RBrace, "期望 '}'")?;

    Ok(SpecializeClassDecl {
        base_name,
        type_args,
        members,
        namespace_path: Vec::new(),
        loc,
    })
}

/// 解析泛型类型参数 <T, U, V, ...>
/// 支持: T, T: Bound, T = Default, T: Bound = Default
pub fn parse_generic_type_params(parser: &mut Parser) -> CayResult<Vec<crate::ast::TypeParam>> {
    let mut params = Vec::new();

    if parser.match_token(&Token::Lt) {
        loop {
            let param_name = parser.consume_identifier(
                "期望泛型类型参数名\n提示: 泛型类型参数应为标识符，例如: <T> 或 <T, U>",
            )?;

            // 可选的类型边界: T: Bound
            let bound = if parser.match_token(&Token::Colon) {
                Some(parser.consume_identifier("期望类型边界名\n提示: 类型边界格式为 'T: Bound'")?)
            } else {
                None
            };

            // 可选的默认类型: T = Default
            let default_type = if parser.match_token(&Token::Assign) {
                Some(super::types::parse_type(parser)?)
            } else {
                None
            };

            params.push(crate::ast::TypeParam {
                name: param_name,
                bound,
                default_type,
            });

            if !parser.match_token(&Token::Comma) {
                break;
            }
        }
        parser.consume(
            &Token::Gt,
            "期望 '>' 结束泛型参数列表\n提示: 泛型类型参数应以 '>' 结束，例如: <T> 或 <K, V>",
        )?;
    }

    Ok(params)
}

/// 解析泛型类型实参 <Type1, Type2, ...>（用于类型使用）
/// 例如：Optional<int, String>
pub fn parse_generic_type_args(parser: &mut Parser) -> CayResult<Vec<Type>> {
    super::types::parse_generic_type_args(parser)
}
