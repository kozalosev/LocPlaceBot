use super::{is_query_correct, COORDS_REGEXP, QUERY_REGEX};

#[test]
fn test_coords_regex() {
    let false_cases = [
        "",
        "  ",
        ".,",
        "!",
        "123",
        "1,2",
        "1.2,3.4",
        "1,2,3,4",
    ];
    let true_cases = [
        "1 2",
        "1.2 3.4",
        "1,2 3,4",
        "1.2  3.4",
        "1,2  3,4",
        "1.2, 3.4",
        "12.345 67.89",
    ];

    run_test(false_cases, true_cases, |case| COORDS_REGEXP.is_match(case))
}

#[test]
fn test_query_regex() {
    let false_cases = [
        "",
        "  ",
        ".,",
        "!",
        "123",
        "1,2",
        "1.2,3.4",
        "1,2,3,4",
        "1 2",
        "1.2 3.4",
        "1,2 3,4",
        "1.2  3.4",
        "1,2  3,4",
        "1.2, 3.4",
        "12.345 67.89",
    ];
    let true_cases = [
        "Avenue",
        "Ave 12",
        "Kremlin, Moscow, Russia",
        "Кремль, Москва, Россия",
        "中国北京",
        "دبي مارينا، دبي، الإمارات العربية المتحدة",
    ];
    
    run_test(false_cases, true_cases, |case| QUERY_REGEX.is_match(case))
}

#[test]
fn test_is_query_correct() {
    let false_cases = [
        "",
        "  ",
        ".,",
        "!",
        "123",
        "1,2",
        "1.2,3.4",
        "1,2,3,4",
    ];
    let true_cases = [
        "Avenue",
        "Ave 12",
        "1 2",
        "1.2 3.4",
        "1,2 3,4",
        "1.2  3.4",
        "1.2, 3.4",
        "12.345 67.89",
        "Kremlin, Moscow, Russia",
        "Кремль, Москва, Россия",
        "中国北京",
        "دبي مارينا، دبي، الإمارات العربية المتحدة",
    ];
    
    run_test(false_cases, true_cases, is_query_correct)
}

fn run_test<const N1: usize, const N2: usize>(
    false_cases: [&str; N1],
    true_cases: [&str; N2],
    runner: fn(&str) -> bool
) {
    let false_cases = false_cases.into_iter().map(|case| (case, false));
    let true_cases  = true_cases.into_iter().map(|case| (case, true));

    for (param, expected) in false_cases.chain(true_cases) {
        assert_eq!(expected, runner(param), "param: '{param}'");
    }
}
