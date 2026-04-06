const configPromise = fetch("/config.json").then((res) => {
  if (!res.ok) throw new Error(`Failed to load /config.json: ${res.status}`);
  return res.json();
});

export default configPromise;
