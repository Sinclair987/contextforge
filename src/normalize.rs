pub(crate) fn normalize_width_and_radicals(ch: char) -> char {
    if let Some(ascii) = normalize_full_width_ascii(ch) {
        return ascii;
    }

    match ch {
        '\u{3000}' => ' ',
        '\u{2EC5}' => '\u{89C1}',
        '\u{2EC9}' => '\u{8D1D}',
        '\u{2ED3}' => '\u{957F}',
        '\u{2EDA}' => '\u{9875}',
        '\u{2EDB}' => '\u{98CE}',
        '\u{2EDC}' => '\u{98DE}',
        '\u{2F00}' => '\u{4E00}',
        '\u{2F06}' => '\u{4E8C}',
        '\u{2F08}' => '\u{4EBA}',
        '\u{2F0A}' => '\u{5165}',
        '\u{2F0B}' => '\u{516B}',
        '\u{2F0F}' => '\u{51E0}',
        '\u{2F12}' => '\u{529B}',
        '\u{2F17}' => '\u{5341}',
        '\u{2F1D}' => '\u{53E3}',
        '\u{2F24}' => '\u{5927}',
        '\u{2F26}' => '\u{5B50}',
        '\u{2F29}' => '\u{5C0F}',
        '\u{2F2F}' => '\u{5DE5}',
        '\u{2F30}' => '\u{5DF1}',
        '\u{2F32}' => '\u{5E72}',
        '\u{2F3C}' => '\u{5FC3}',
        '\u{2F3E}' => '\u{6236}',
        '\u{2F40}' => '\u{652F}',
        '\u{2F42}' => '\u{6587}',
        '\u{2F45}' => '\u{65B9}',
        '\u{2F46}' => '\u{65E0}',
        '\u{2F47}' => '\u{65E5}',
        '\u{2F49}' => '\u{6708}',
        '\u{2F4C}' => '\u{6B62}',
        '\u{2F50}' => '\u{6BD4}',
        '\u{2F54}' => '\u{6C34}',
        '\u{2F55}' => '\u{706B}',
        '\u{2F5A}' => '\u{7247}',
        '\u{2F63}' => '\u{751F}',
        '\u{2F64}' => '\u{7528}',
        '\u{2F6C}' => '\u{76EE}',
        '\u{2F70}' => '\u{793A}',
        '\u{2F74}' => '\u{7ACB}',
        '\u{2F79}' => '\u{7F51}',
        '\u{2F7D}' => '\u{800C}',
        '\u{2F7F}' => '\u{8033}',
        '\u{2F83}' => '\u{81EA}',
        '\u{2F84}' => '\u{81F3}',
        '\u{2F8D}' => '\u{866B}',
        '\u{2F8F}' => '\u{884C}',
        '\u{2F92}' => '\u{898B}',
        '\u{2F94}' => '\u{8A00}',
        '\u{2F9C}' => '\u{8DB3}',
        '\u{2FA5}' => '\u{91CC}',
        '\u{2FAE}' => '\u{975E}',
        '\u{2FAF}' => '\u{9762}',
        '\u{2FB3}' => '\u{97F3}',
        '\u{2FBC}' => '\u{9AD8}',
        '\u{2FCE}' => '\u{9F13}',
        _ => ch,
    }
}

pub(crate) fn normalize_search_char(ch: char) -> char {
    match normalize_width_and_radicals(ch) {
        '，' | '、' | '。' | '：' | '；' | '！' | '？' | '（' | '）' | '【' | '】' | '《'
        | '》' | '“' | '”' | '‘' | '’' | '—' | '…' | '·' => ' ',
        normalized => normalized,
    }
}

fn normalize_full_width_ascii(ch: char) -> Option<char> {
    let code = ch as u32;
    if !(0xFF01..=0xFF5E).contains(&code) {
        return None;
    }

    char::from_u32(code - 0xFEE0)
}
