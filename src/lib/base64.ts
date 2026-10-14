/** base64 <-> bytes helpers (browser side). */

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

export function base64ToText(b64: string): string {
  return new TextDecoder().decode(base64ToBytes(b64));
}

const encoder = new TextEncoder();

export function textToBase64(text: string): string {
  return bytesToBase64(encoder.encode(text));
}
