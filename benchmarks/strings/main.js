// String processing benchmark: concat + search
const unit = "hello world ";
const s = unit.repeat(10000);
let c = 0;
let idx = 0;
while ((idx = s.indexOf("world", idx)) !== -1) { c++; idx++; }
console.log(`string_len=${s.length} occurrences=${c}`);
