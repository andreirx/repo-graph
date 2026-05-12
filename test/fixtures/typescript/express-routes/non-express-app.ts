// FD-1A validation: NEGATIVE case - non-Express app.get should NOT be detected
// This file does NOT import express

// Map has a .get() method - should NOT detect
const cache = new Map<string, any>();
cache.get('/api/users');
cache.set('/api/users', { data: [] });

// Custom object with .get() - should NOT detect
const customApp = {
  get(path: string) { return path; },
  post(path: string) { return path; },
};
customApp.get('/custom/route');
customApp.post('/custom/route');

// localStorage has .get - should NOT detect (if it exists)
const storage = {
  get(key: string) { return key; },
};
storage.get('/storage/key');
