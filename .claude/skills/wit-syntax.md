# WIT Syntax Reference

## Package Declaration

```wit
package namespace:name@version;
```

## Keyword Escaping

WIT keywords (`option`, `result`, `list`, `bool`, `char`, `string`, `u32`, `u64`, `i32`, `i64`, `f32`, `f64`, `tuple`, `enum`, `variant`, `record`, `flags`, `resource`, `func`, `type`, `use`, `import`, `export`, `interface`, `world`, `own`, `borrow`, etc.) can be used as identifiers by prefixing with `%`:

```wit
enum primitive-type {
  %u32,
  %u64,
  %bool,
  %string,
}

variant my-variant {
  %option(u32),
  %result(string),
}

record my-record {
  %result: option<string>,
}
```

## Use Statements

Within the same package (bare form):
```wit
interface types {
  type my-type = string;
}

interface api {
  use types.{my-type};
  process: func(input: my-type) -> string;
}
```

Cross-package:
```wit
use wasi:http/types@1.0.0 as http-types;
```

## Multiple Interfaces and Worlds

A single .wit file can contain multiple interfaces and worlds. Only one `package` declaration is needed per directory.

## Worlds

```wit
world my-world {
  import some-interface;
  export another-interface;
}
```

With WASI:
```wit
world my-world {
  import wasi:filesystem/types@0.2.0;
  export my-interface;
}
```

## Built-in Types

- Primitives: `u8`, `u16`, `u32`, `u64`, `s8`, `s16`, `s32`, `s64`, `f32`, `f64`, `bool`, `char`, `string`
- Containers: `list<T>`, `option<T>`, `result<T, E>`, `result<_, E>`, `result<T>`, `result`
- Compound: `tuple<T1, T2, ...>`
- Handle: `own<T>`, `borrow<T>`

## Type Definitions

```wit
type my-alias = string;

record my-record {
  field1: u32,
  field2: string,
}

variant my-variant {
  case-a(u32),
  case-b(string),
  case-c,
}

enum my-enum {
  value-a,
  value-b,
}

flags my-flags {
  read,
  write,
  execute,
}
```
