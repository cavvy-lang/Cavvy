//! 类型解析

use super::Parser;
use crate::error::cayResult;
use crate::types::Type;

/// 解析类型（支持多维数组和指针，以及类型别名）
pub fn parse_type(parser: &mut Parser) -> cayResult<Type> {
    let (ty, extra_generic_closers) = parse_type_inner(parser)?;
    if extra_generic_closers > 0 {
        return Err(
            parser.error("泛型类型实参列表中多余的 '>'\n提示: 请检查泛型类型的尖括号是否匹配")
        );
    }

    Ok(ty)
}

/// 解析泛型类型实参 <Type1, Type2, ...>（用于类型使用）
/// 支持嵌套泛型结尾被词法器合并为 >> / >>> 的情况。
pub fn parse_generic_type_args(parser: &mut Parser) -> cayResult<Vec<Type>> {
    let (args, extra_generic_closers) = parse_generic_type_args_inner(parser)?;
    if extra_generic_closers > 0 {
        return Err(
            parser.error("泛型类型实参列表中多余的 '>'\n提示: 请检查泛型类型的尖括号是否匹配")
        );
    }

    Ok(args)
}

fn parse_type_inner(parser: &mut Parser) -> cayResult<(Type, usize)> {
    let base_type = match parser.current_token() {
        crate::lexer::Token::Int => {
            parser.advance();
            Type::Int32
        }
        crate::lexer::Token::Long => {
            parser.advance();
            Type::Int64
        }
        crate::lexer::Token::Float => {
            parser.advance();
            Type::Float32
        }
        crate::lexer::Token::Double => {
            parser.advance();
            Type::Float64
        }
        crate::lexer::Token::Bool => {
            parser.advance();
            Type::Bool
        }
        crate::lexer::Token::String => {
            parser.advance();
            Type::String
        }
        crate::lexer::Token::Char => {
            parser.advance();
            Type::Char
        }
        crate::lexer::Token::Void => {
            parser.advance();
            Type::Void
        }
        // FFI 类型
        crate::lexer::Token::CInt => {
            parser.advance();
            Type::CInt
        }
        crate::lexer::Token::CUInt => {
            parser.advance();
            Type::CUInt
        }
        crate::lexer::Token::CLong => {
            parser.advance();
            Type::CLong
        }
        crate::lexer::Token::CULong => {
            parser.advance();
            Type::CULong
        }
        crate::lexer::Token::CShort => {
            parser.advance();
            Type::CShort
        }
        crate::lexer::Token::CUShort => {
            parser.advance();
            Type::CUShort
        }
        crate::lexer::Token::CChar => {
            parser.advance();
            Type::CChar
        }
        crate::lexer::Token::CUChar => {
            parser.advance();
            Type::CUChar
        }
        crate::lexer::Token::CFloat => {
            parser.advance();
            Type::CFloat
        }
        crate::lexer::Token::CDouble => {
            parser.advance();
            Type::CDouble
        }
        crate::lexer::Token::SizeT => {
            parser.advance();
            Type::SizeT
        }
        crate::lexer::Token::SSizeT => {
            parser.advance();
            Type::SSizeT
        }
        crate::lexer::Token::UIntPtr => {
            parser.advance();
            Type::UIntPtr
        }
        crate::lexer::Token::IntPtr => {
            parser.advance();
            Type::IntPtr
        }
        crate::lexer::Token::CVoid => {
            parser.advance();
            Type::CVoid
        }
        crate::lexer::Token::CBool => {
            parser.advance();
            Type::CBool
        }
        // c_string 是 *c_char 的别名
        crate::lexer::Token::CString => {
            parser.advance();
            Type::Pointer(Box::new(Type::CChar))
        }
        crate::lexer::Token::CInt64 => {
            parser.advance();
            Type::Int64
        }
        crate::lexer::Token::CUInt64 => {
            parser.advance();
            Type::Int64
        }
        crate::lexer::Token::Identifier(name) => {
            let mut name = name.clone();
            parser.advance();
            // 支持命名空间限定类型名: std::StringBuilder, std::io::File
            while parser.check(&crate::lexer::Token::DoubleColon) {
                parser.advance(); // 消费 ::
                let next = parser.consume_identifier(
                    "期望命名空间后的标识符\n提示: 限定类型名格式为 'ns::Type' 或 'ns::sub::Type'",
                )?;
                name = format!("{}::{}", name, next);
            }
            // 检查是否是已定义的类型别名
            if let Some(aliased_type) = parser.get_type_alias(&name) {
                aliased_type
            } else {
                // 检查是否有泛型类型参数 <...>
                let type_name = Type::Object(name.clone());
                if parser.check(&crate::lexer::Token::Lt) {
                    // 解析泛型类型参数: Optional<int, String>
                    let (type_args, extra_generic_closers) = parse_generic_type_args_inner(parser)?;
                    if extra_generic_closers > 0 {
                        return Ok((Type::Generic(name, type_args), extra_generic_closers));
                    }
                    Type::Generic(name, type_args)
                } else {
                    type_name
                }
            }
        }
        _ => {
            let current_token = parser.current_token();
            let (token_desc, suggestion) = match current_token {
                // 分隔符
                crate::lexer::Token::Semicolon => (
                    "分号(;)".to_string(),
                    "分号不能作为类型。可能的问题:\n    - 变量声明中类型后缺少变量名，如: int; 应该是 int x;\n    - 多余的逗号或分号".to_string()
                ),
                crate::lexer::Token::Comma => (
                    "逗号(,)".to_string(),
                    "逗号不能作为类型。可能的问题:\n    - 参数列表中多余的逗号，如: int x, , int y 应该是 int x, int y\n    - 类型声明位置错误".to_string()
                ),
                crate::lexer::Token::LParen => (
                    "左圆括号(()".to_string(),
                    "括号不能作为类型。可能的问题:\n    - 类型声明位置错误\n    - 函数指针语法需要使用特殊语法".to_string()
                ),
                crate::lexer::Token::RParen => (
                    "右圆括号())".to_string(),
                    "括号不能作为类型。可能的问题:\n    - 参数列表提前结束\n    - 缺少类型声明".to_string()
                ),
                crate::lexer::Token::LBrace => (
                    "左花括号({)".to_string(),
                    "代码块开始不能作为类型。可能的问题:\n    - 类型声明位置错误\n    - 函数体提前开始".to_string()
                ),
                crate::lexer::Token::RBrace => (
                    "右花括号(})".to_string(),
                    "代码块结束不能作为类型。可能的问题:\n    - 前面的声明缺少类型\n    - 代码块提前结束".to_string()
                ),
                crate::lexer::Token::LBracket => (
                    "左方括号([)".to_string(),
                    "方括号不能作为类型开始。可能的问题:\n    - 数组类型声明顺序错误，如: []int 应该是 int[]\n    - 数组字面量位置错误".to_string()
                ),
                crate::lexer::Token::RBracket => (
                    "右方括号(])".to_string(),
                    "方括号不能作为类型。可能的问题:\n    - 数组类型声明中缺少类型，如: [] arr 应该是 int[] arr\n    - 多余的右方括号".to_string()
                ),
                // 关键字
                crate::lexer::Token::Class => (
                    "关键字(class)".to_string(),
                    "class 是类声明关键字，不是类型。可能的问题:\n    - 类声明位置错误\n    - 需要类名作为类型时使用了 class 关键字".to_string()
                ),
                crate::lexer::Token::Interface => (
                    "关键字(interface)".to_string(),
                    "interface 是接口声明关键字，不是类型。可能的问题:\n    - 接口声明位置错误".to_string()
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
                        format!("{} 是控制流关键字，不能作为类型。可能的问题:\n    - 类型声明位置错误\n    - 语句位置错误", kw)
                    )
                }
                // 修饰符
                crate::lexer::Token::Public | crate::lexer::Token::Private |
                crate::lexer::Token::Protected | crate::lexer::Token::Static |
                crate::lexer::Token::Final | crate::lexer::Token::Abstract => {
                    let kw = format!("{:?}", current_token).to_lowercase();
                    (
                        format!("关键字({})", kw),
                        format!("{} 是修饰符，不能作为类型。可能的问题:\n    - 修饰符顺序错误\n    - 类型声明位置错误", kw)
                    )
                }
                // 字面量
                crate::lexer::Token::IntegerLiteral(Some((val, _))) => (
                    format!("整数({})", val),
                    "整数字面量不能作为类型。可能的问题:\n    - 数组大小声明位置错误，如: int[10] arr 应该是 int[] arr = new int[10]\n    - 类型声明位置错误".to_string()
                ),
                crate::lexer::Token::StringLiteral(Some(s)) => (
                    format!("字符串(\"{}\")", s),
                    "字符串字面量不能作为类型。可能的问题:\n    - 类型声明位置错误\n    - 字符串应作为值使用".to_string()
                ),
                // 标识符（可能是未定义的类名）
                crate::lexer::Token::Identifier(name) => {
                    let name_owned = name.clone();
                    (
                        format!("标识符('{}')", name_owned),
                        format!("'{}' 不是已知的类型。可能的问题:\n    - 类名拼写错误\n    - 缺少 import 或 using\n    - 需要先定义类 '{}' 再使用", name_owned, name_owned)
                    )
                }
                // 其他
                _ => {
                    let token_name = super::utils::get_token_name(current_token);
                    (
                        token_name.clone(),
                        format!("{} 不能作为类型。请使用有效的类型名称。", token_name)
                    )
                }
            };
            return Err(parser.error(&format!(
                "期望类型，但遇到了 {}\n提示: {}",
                token_desc, suggestion
            )));
        }
    };

    // 检查指针类型 Type*（支持多级指针 Type**）
    let mut result_type = base_type;
    while parser.match_token(&crate::lexer::Token::Star) {
        result_type = Type::Pointer(Box::new(result_type));
    }

    // 检查多维数组类型 Type[][]...
    while parser.match_token(&crate::lexer::Token::LBracket) {
        parser.consume(
            &crate::lexer::Token::RBracket,
            "期望 ']'\n提示: 数组类型声明应为 Type[]，例如: int[]",
        )?;
        result_type = Type::Array(Box::new(result_type));
    }

    Ok((result_type, 0))
}

fn parse_generic_type_args_inner(parser: &mut Parser) -> cayResult<(Vec<Type>, usize)> {
    let mut args = Vec::new();

    parser.consume(
        &crate::lexer::Token::Lt,
        "期望 '<' 开始泛型类型实参\n提示: 泛型类型实参应以 '<' 开始，例如: Optional<int>",
    )?;

    loop {
        let (arg_type, extra_generic_closers) = parse_type_inner(parser)?;
        args.push(arg_type);

        if extra_generic_closers > 0 {
            return Ok((args, extra_generic_closers - 1));
        }

        if parser.match_token(&crate::lexer::Token::Comma) {
            continue;
        }

        let extra_generic_closers = consume_generic_type_close(parser)?;
        return Ok((args, extra_generic_closers));
    }
}

fn consume_generic_type_close(parser: &mut Parser) -> cayResult<usize> {
    match parser.current_token() {
        crate::lexer::Token::Gt => {
            parser.advance();
            Ok(0)
        }
        crate::lexer::Token::Shr => {
            parser.advance();
            Ok(1)
        }
        crate::lexer::Token::UnsignedShr => {
            parser.advance();
            Ok(2)
        }
        _ => {
            parser.consume(
                &crate::lexer::Token::Gt,
                "期望 '>' 结束泛型类型实参\n提示: 泛型类型实参应以 '>' 结束，例如: Optional<int>",
            )?;
            Ok(0)
        }
    }
}

/// 检查当前token是否是类型token
pub fn is_type_token(parser: &Parser) -> bool {
    matches!(
        parser.current_token(),
        crate::lexer::Token::Int | crate::lexer::Token::Long | crate::lexer::Token::Float |
        crate::lexer::Token::Double | crate::lexer::Token::Bool | crate::lexer::Token::String |
        crate::lexer::Token::Char | crate::lexer::Token::Void | crate::lexer::Token::Identifier(_) |
        // FFI 类型
        crate::lexer::Token::CInt | crate::lexer::Token::CUInt | crate::lexer::Token::CLong |
        crate::lexer::Token::CULong | crate::lexer::Token::CShort | crate::lexer::Token::CUShort |
        crate::lexer::Token::CChar | crate::lexer::Token::CUChar |
        crate::lexer::Token::CFloat | crate::lexer::Token::CDouble |
        crate::lexer::Token::SizeT | crate::lexer::Token::SSizeT | crate::lexer::Token::UIntPtr |
        crate::lexer::Token::IntPtr | crate::lexer::Token::CVoid | crate::lexer::Token::CBool |
        crate::lexer::Token::CString | crate::lexer::Token::CInt64 | crate::lexer::Token::CUInt64
    )
}

/// 检查当前token是否是原始类型token
pub fn is_primitive_type_token(parser: &Parser) -> bool {
    matches!(
        parser.current_token(),
        crate::lexer::Token::Int
            | crate::lexer::Token::Long
            | crate::lexer::Token::Float
            | crate::lexer::Token::Double
            | crate::lexer::Token::Bool
            | crate::lexer::Token::String
            | crate::lexer::Token::Char
    )
}
