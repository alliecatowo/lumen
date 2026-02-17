function main(): void {
  let s = "";
  for (let i = 0; i < 100000; i++) {
    s += "x";
  }
  console.log(`Length: ${s.length}`);
}

main();
