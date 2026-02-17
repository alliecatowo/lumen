function main(): void {
  // Build an object with 10000 entries
  const data: Record<string, string> = {};
  for (let i = 0; i < 10000; i++) {
    data[`key_${i}`] = `value_${i}`;
  }

  // Serialize to JSON
  const jsonStr = JSON.stringify(data);

  // Parse back
  const parsed = JSON.parse(jsonStr) as Record<string, string>;

  // Access a field
  const found = parsed["key_9999"];
  console.log(`Found: ${found}`);
  console.log(`Count: ${Object.keys(parsed).length}`);
}

main();
