pub fn format_id_expr(expr: &str, idx: usize) -> Result<String, String> {
    let value = eval_id_expr(expr, idx)?;
    Ok(format!("{:08X}", value))
}

pub fn format_repeat_name(
    template: Option<&str>,
    id_value: Option<&str>,
    idx: usize,
    count: usize,
) -> String {
    if let Some(template) = template {
        let mut s = template.replace("{index0}", &idx.to_string());
        s = s.replace("{index}", &(idx + 1).to_string());
        s = s.replace("{count}", &count.to_string());
        if let Some(id_str) = id_value {
            if s.contains("{id}") {
                s = s.replace("{id}", id_str);
            } else {
                s = format!("{}_{}", id_str, s);
            }
        }
        s
    } else if let Some(id_str) = id_value {
        id_str.to_string()
    } else {
        String::new()
    }
}

pub fn eval_id_expr(expr: &str, idx: usize) -> Result<u32, String> {
    struct Parser<'a> {
        input: &'a str,
        pos: usize,
        idx: usize,
    }

    impl<'a> Parser<'a> {
        fn new(input: &'a str, idx: usize) -> Self {
            Parser { input, pos: 0, idx }
        }

        fn peek(&self) -> Option<char> {
            self.input[self.pos..].chars().next()
        }

        fn next(&mut self) -> Option<char> {
            if let Some(ch) = self.peek() {
                self.pos += ch.len_utf8();
                Some(ch)
            } else {
                None
            }
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
                self.next();
            }
        }

        fn parse_number(&mut self) -> Result<u32, String> {
            self.skip_ws();
            let start = self.pos;
            if self.input[self.pos..].starts_with("0x") || self.input[self.pos..].starts_with("0X") {
                self.pos += 2;
                while matches!(self.peek(), Some(ch) if ch.is_ascii_hexdigit()) {
                    self.next();
                }
                let token = &self.input[start + 2..self.pos];
                u32::from_str_radix(token, 16).map_err(|_| format!("非法十六进制数: {}", token))
            } else {
                while matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
                    self.next();
                }
                let token = &self.input[start..self.pos];
                token
                    .parse::<u32>()
                    .map_err(|_| format!("非法十进制数: {}", token))
            }
        }

        fn parse_variable(&mut self) -> Result<u32, String> {
            self.skip_ws();
            let start = self.pos;
            while matches!(self.peek(), Some(ch) if ch.is_ascii_alphanumeric() || ch == '_') {
                self.next();
            }
            let token = &self.input[start..self.pos];
            match token {
                "index" => Ok((self.idx + 1) as u32),
                "index0" => Ok(self.idx as u32),
                _ => Err(format!("非法变量: {}", token)),
            }
        }

        fn parse_factor(&mut self) -> Result<u32, String> {
            self.skip_ws();
            if self.input[self.pos..].starts_with('(') {
                self.next();
                let value = self.parse_expr()?;
                self.skip_ws();
                if self.next() != Some(')') {
                    return Err("缺少 )".to_string());
                }
                Ok(value)
            } else if self.input[self.pos..].starts_with("index0") {
                self.parse_variable()
            } else if self.input[self.pos..].starts_with("index") {
                self.parse_variable()
            } else if matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
                self.parse_number()
            } else {
                Err(format!(
                    "非法表达式起始: {}",
                    self.input[self.pos..].chars().next().unwrap_or('?')
                ))
            }
        }

        fn parse_term(&mut self) -> Result<u32, String> {
            let mut value = self.parse_factor()? as u64;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('*') => {
                        self.next();
                        let rhs = self.parse_factor()? as u64;
                        value = value.checked_mul(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    Some('/') => {
                        self.next();
                        let rhs = self.parse_factor()? as u64;
                        if rhs == 0 {
                            return Err("除以零".to_string());
                        }
                        value = value.checked_div(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    _ => break,
                }
            }
            Ok(value as u32)
        }

        fn parse_expr(&mut self) -> Result<u32, String> {
            let mut value = self.parse_term()? as i64;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('+') => {
                        self.next();
                        let rhs = self.parse_term()? as i64;
                        value = value.checked_add(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    Some('-') => {
                        self.next();
                        let rhs = self.parse_term()? as i64;
                        value = value.checked_sub(rhs).ok_or_else(|| "溢出".to_string())?;
                    }
                    _ => break,
                }
            }
            if value < 0 || value > u32::MAX as i64 {
                Err("表达式结果越界".to_string())
            } else {
                Ok(value as u32)
            }
        }
    }

    let mut parser = Parser::new(expr, idx);
    let result = parser.parse_expr()?;
    parser.skip_ws();
    if parser.peek().is_some() {
        return Err(format!("未处理的表达式尾部: {}", &expr[parser.pos..]));
    }
    Ok(result)
}
