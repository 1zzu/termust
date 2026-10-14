# Format penyimpanan vault v1

Dokumen ini menjelaskan **format penyimpanan dan versioning** vault Termust, bukan dasar teori
kriptografi. Phase 1A memakai dua algoritma standar tanpa primitif buatan sendiri:

- **Argon2id** untuk menurunkan Key Encryption Key dari master password.
- **XChaCha20-Poly1305** untuk membungkus Vault Key dan mengenkripsi payload object.

## Lokasi

Satu vault adalah satu database SQLite bernama `termust.db` di application data directory milik
sistem operasi (lihat bagian _Local vault_ di README). Database tidak pernah disimpan di working
directory atau di dalam repositori.

## Rantai kunci

```text
master password
      │  Argon2id (salt acak 16 byte, parameter tersimpan di vault_meta)
      ▼
Key Encryption Key (32 byte)
      │  XChaCha20-Poly1305, domain AAD "vault-key"
      ▼
Vault Key (32 byte acak)
      │  XChaCha20-Poly1305, domain AAD "object"
      ▼
payload object
```

- Salt Argon2id dibuat acak per vault dan disimpan bersama parameter biayanya.
- Vault Key dibuat dari sumber acak sistem operasi dan hanya pernah disimpan dalam bentuk
  terbungkus (wrapped).
- Setiap enkripsi memakai nonce 24 byte yang baru. Nonce tidak pernah dipakai ulang dengan key
  dan konteks yang sama.

## Skema SQLite

Versi skema disimpan di `PRAGMA user_version`. Migrasi memakai `CREATE TABLE IF NOT EXISTS`,
jadi menjalankannya berulang aman.

### `vault_meta` — tepat satu row

| Kolom                      | Tipe    | Isi                                                      |
| -------------------------- | ------- | -------------------------------------------------------- |
| `id`                       | TEXT    | UUID vault, dibuat aplikasi; bagian dari AAD wrapped key |
| `schema_version`           | INTEGER | versi skema saat row ditulis                             |
| `crypto_version`           | INTEGER | versi format crypto                                      |
| `kdf_algorithm`            | TEXT    | `argon2id`                                               |
| `kdf_salt`                 | BLOB    | salt Argon2id (16 byte)                                  |
| `kdf_memory_cost`          | INTEGER | biaya memori Argon2id (KiB)                              |
| `kdf_time_cost`            | INTEGER | jumlah pass Argon2id                                     |
| `kdf_parallelism`          | INTEGER | derajat paralelisme Argon2id                             |
| `wrapped_vault_key`        | BLOB    | Vault Key terenkripsi                                    |
| `wrap_nonce`               | BLOB    | nonce untuk `wrapped_vault_key`                          |
| `created_at`, `updated_at` | INTEGER | Unix time detik (untuk tampilan)                         |

### `vault_objects`

| Kolom                      | Tipe         | Isi                                 |
| -------------------------- | ------------ | ----------------------------------- |
| `id`                       | TEXT         | UUID object, dibuat aplikasi        |
| `kind`                     | TEXT         | jenis object; bagian dari AAD       |
| `encrypted_payload`        | BLOB         | envelope yang terenkripsi           |
| `nonce`                    | BLOB         | nonce XChaCha20-Poly1305 object     |
| `crypto_version`           | INTEGER      | versi crypto saat baris ini ditulis |
| `created_at`, `updated_at` | INTEGER      | Unix time detik                     |
| `deleted_at`               | INTEGER NULL | tombstone; `NULL` berarti aktif     |

Delete memakai tombstone: baris tetap ada dan `deleted_at` diisi, sehingga penghapusan tidak
langsung menghilangkan data dan dapat disinkronkan pada fase berikutnya.

## Associated Data

AAD dibangun deterministik dari bagian-bagian yang diberi panjang, supaya dua input berbeda tidak
pernah menghasilkan byte yang sama:

```text
"termust" ‖ 0x00 ‖ crypto_version (i64 little-endian) ‖ 0x00 ‖ domain ‖ (len(part) u64 ‖ part)*
```

| Konteks           | `domain`    | `part`              |
| ----------------- | ----------- | ------------------- |
| Wrapped Vault Key | `vault-key` | `id` vault          |
| Payload object    | `object`    | `id` object, `kind` |

Domain yang berbeda membuat ciphertext tidak dapat dipindahkan antar-konteks: ciphertext object
tidak akan diterima sebagai wrapped Vault Key, dan sebaliknya. Mengubah `id`, `kind`, nonce, atau
ciphertext membuat dekripsi gagal.

## Envelope payload

Sebelum dienkripsi, payload dibungkus JSON dengan versi sendiri:

```json
{ "schemaVersion": 1, "kind": "note", "payload": "{\"bebas\":true}" }
```

Versioning ini memisahkan format payload dari layout SQLite: perubahan format payload dapat
dimigrasikan dengan mendekripsi, membaca `schemaVersion`, lalu menulis ulang.

## Versioning

Dua nomor versi bergerak sendiri-sendiri:

- **`crypto_version`** dinaikkan bila cara membungkus Vault Key atau mengenkripsi payload berubah.
- **`user_version` / `schema_version`** dinaikkan bila layout SQLite berubah; migrasi berikutnya
  ditambahkan sebagai langkah baru, bukan sebagai penulisan ulang.

Aturan yang berlaku saat ini:

- Vault dengan versi yang lebih tinggi daripada yang didukung build ditolak dengan pesan yang
  jelas, dan alasan itu dibedakan dari password yang salah.
- Karena hanya versi 1 yang ada, vault versi 1 selalu diterima; versi lama harus dimigrasikan,
  tidak diganti.
- Parameter Argon2id disimpan per vault supaya default baru tidak mengunci vault lama.

## Yang tidak pernah disimpan

- Master password, dan tidak ada hash master password sebagai mekanisme unlock.
- Vault Key dalam bentuk plaintext (hanya wrapped).
- Payload object dalam bentuk plaintext.
- Metadata yang dapat dicari: nama host, username, nama credential, fingerprint, serta nama dan isi
  snippet seluruhnya berada di dalam payload terenkripsi. Plaintext di `vault_objects` hanya `id`,
  `kind`, nonce, `crypto_version`, dan timestamp.

## Pemulihan

Tidak ada jalur pemulihan pada Phase 1A: tanpa master password, isi vault tidak dapat dibuka.
Recovery key, keychain, dan biometric unlock belum ada dan berada di luar scope fase ini.
