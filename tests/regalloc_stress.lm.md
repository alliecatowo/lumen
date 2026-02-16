# RegAlloc Stress Test

This test pushes the limits of temporary register allocation and recycling to ensure that contiguous blocks are correctly reserved even when the free pool is fragmented.

```lumen
cell main() -> list[Int]
  // Create many temporary values to fill the free list and fragmentation
  let a = 1
  let b = 2
  let c = 3
  let d = 4
  let e = 5
  
  // Use expressions that generate many intermediate temps
  let list_one = [a + 1, b + 2, c + 3, d + 4, e + 5]
  
  // More temps
  let f = 6
  let g = 7
  let h = 8
  
  // Large literal with internal expressions
  let complex_list = [
    a + b + c + d + e,
    f * g * h,
    [a, b, c].len(),
    { "key": "val" }.len(),
    (a + b) * (c + d)
  ]
  
  return complex_list
end
```
