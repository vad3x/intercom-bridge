import { expect, test } from 'bun:test';

test('embedded dashboard includes shared shell layout utilities', async () => {
  // Arrange
  const dashboard = Bun.file(new URL('../../assets/dashboard.html', import.meta.url));

  // Act
  const html = await dashboard.text();
  const styles = [...html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/g)]
    .map((match) => match[1])
    .join('\n');

  // Assert
  expect(styles).toMatch(/\.min-h-screen\s*\{[^}]*min-height:\s*100vh/);
  expect(styles).toMatch(/\.max-w-4xl\s*\{[^}]*max-width:/);
});
