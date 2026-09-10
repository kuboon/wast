// Playground samples, ordered roughly by how much of the compiler they ask
// for. Each one is a complete `(component …)` — the playground parses it with
// the `raw` syntax plugin, compiles it to a core wasm module, and (where the
// signature allows) instantiates and calls it right in the page.
//
// `calls` drives the "Run" panel: each entry names an export and the argument
// values to pass. Only exports whose params and result each occupy a single
// core value are callable without glue; the ones that need the Canonical ABI
// (string, record, …) are marked with `abi` so the page can say why it is
// showing raw memory instead of a plain value.
//
// `packages/web-demo/test/samples.test.mjs` compiles every sample and runs
// every call listed here, so this file doubles as the compiler's coverage
// record: if a sample stops compiling, that test fails.

export const SAMPLES = [
  {
    id: "arithmetic",
    label: "Arithmetic & calls",
    blurb:
      "Integer arithmetic and calls between functions. Everything here is a single core value, so the exports are callable straight from JS.",
    calls: [
      { func: "cube", args: [3] },
      { func: "poly", args: [4] },
    ],
    source: `(component
  (func $square (internal $square) (param $x $u32) (result $u32)
    (i64.mul (local.get $x) (local.get $x)))

  (func $cube (export $cube) (param $x $u32) (result $u32)
    (i64.mul (local.get $x) (call $square (local.get $x))))

  ;; Call arguments bind positionally. The renderer writes each one's
  ;; parameter uid as a (; $x ;) comment, but writing them is optional.
  (func $poly (export $poly) (param $x $u32) (result $u32)
    (i64.add (call $square (local.get $x)) (call $cube (local.get $x))))
)`,
  },

  {
    id: "branching",
    label: "Branching",
    blurb:
      "`if` / `then` / `else` as an expression, plus comparisons. `max3` nests calls to `max2`.",
    calls: [
      { func: "max2", args: [11, 4] },
      { func: "max3", args: [3, 9, 6] },
      { func: "abs-diff", args: [4, 10] },
    ],
    source: `(component
  (func $max2 (export $max2) (param $a $u32) (param $b $u32) (result $u32)
    (if (i64.gt (local.get $a) (local.get $b))
      (then (local.get $a))
      (else (local.get $b))))

  (func $max3 (export $max3) (param $a $u32) (param $b $u32) (param $c $u32) (result $u32)
    (call $max2 (call $max2 (local.get $a) (local.get $b)) (local.get $c)))

  (func $abs_diff (export $abs-diff) (param $a $u32) (param $b $u32) (result $u32)
    (if (i64.gt (local.get $a) (local.get $b))
      (then (i64.sub (local.get $a) (local.get $b)))
      (else (i64.sub (local.get $b) (local.get $a)))))
)`,
  },

  {
    id: "loop",
    label: "Loops & locals",
    blurb:
      "`local.set` introduces a local; `block` / `loop` / `br_if` build the loop. `sum-to` adds 1..n, `factorial` multiplies them.",
    calls: [
      { func: "sum-to", args: [10] },
      { func: "factorial", args: [5] },
    ],
    source: `(component
  (func $sum_to (export $sum-to) (param $n $u32) (result $u32)
    (local.set $acc (i64.const 0))
    (local.set $i (i64.const 1))
    (block $done
      (loop $again
        (br_if $done (i64.gt (local.get $i) (local.get $n)))
        (local.set $acc (i64.add (local.get $acc) (local.get $i)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $again)))
    (local.get $acc))

  (func $factorial (export $factorial) (param $n $u32) (result $u32)
    (local.set $acc (i64.const 1))
    (local.set $i (i64.const 2))
    (block $done
      (loop $again
        (br_if $done (i64.gt (local.get $i) (local.get $n)))
        (local.set $acc (i64.mul (local.get $acc) (local.get $i)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $again)))
    (local.get $acc))
)`,
  },

  {
    id: "option",
    label: "option<T>",
    blurb:
      "A compound parameter. `option<u32>` arrives flattened by the Canonical ABI into a discriminant plus a payload, so from JS you pass two values: 0/1 and the payload.",
    calls: [
      { func: "unwrap-or", args: [1, 42, 7], note: "some(42), default 7" },
      { func: "unwrap-or", args: [0, 0, 7], note: "none, default 7" },
    ],
    source: `(component
  (type $opt_u32 (internal $opt_u32) (option $u32))

  (func $unwrap_or (export $unwrap-or) (param $o (option $u32)) (param $default $u32) (result $u32)
    (match_option (local.get $o)
      (some $v (local.get $v))
      (none (local.get $default))))
)`,
  },

  {
    id: "record",
    label: "record",
    blurb:
      "Records are laid out in linear memory. A record parameter arrives as its fields flattened; a record return is written through a caller-supplied pointer, so `make-point` takes an extra address argument.",
    calls: [{ func: "get-x", args: [3, 4], note: "point{x:3, y:4}" }],
    source: `(component
  (type $point (internal $point) (record (field $x $u32) (field $y $u32)))

  (func $get_x (export $get-x) (param $p (record (field $x $u32) (field $y $u32))) (result $u32)
    (record.get $x (local.get $p)))

  (func $make_point (export $make-point) (param $x $u32) (param $y $u32)
        (result (record (field $x $u32) (field $y $u32)))
    (record.literal (; $x ;) (local.get $x) (; $y ;) (local.get $y)))
)`,
  },

  {
    id: "string",
    label: "string",
    blurb:
      "A returned string is a (pointer, length) pair written into the module's own memory. The page reads those bytes back out of `memory` — about twenty lines of glue, no jco.",
    calls: [{ func: "greeting", args: [], abi: "string" }],
    source: `(component
  (func $greeting (export $greeting) (result $string)
    (string.literal "hello, wast!"))

  (func $shout (export $shout) (result $string)
    (string.literal "COMPILED IN YOUR BROWSER"))
)`,
  },
];

export const DEFAULT_SAMPLE = SAMPLES[0].id;
