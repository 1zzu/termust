# Model data lokal v1

Dokumen ini menjelaskan Group, Host, dan Credential pada Phase 1B1: bentuk data, object kind,
versi payload, relasi, dan aturan delete. Format penyimpanan dan crypto-nya tidak berubah dan
tetap dijelaskan di [`vault-format-v1.md`](vault-format-v1.md).

## Tempat penyimpanan

Tidak ada tabel baru. Setiap record adalah satu row `vault_objects` dengan salah satu `kind`:

| Record     | `kind`       |
| ---------- | ------------ |
| Group      | `group`      |
| Host       | `host`       |
| Credential | `credential` |

`kind` ikut menjadi bagian Additional Authenticated Data, jadi sebuah object tidak dapat dibaca
sebagai kind lain: ciphertext group tidak akan pernah bisa diparse sebagai Host. Enkripsi, nonce,
dan tombstone memakai jalur Phase 1A yang sama, satu object per record.

## Payload version

Setiap payload terenkripsi memiliki `version` (`1` saat ini). Payload dengan versi yang tidak
dikenal ditolak dengan pesan yang jelas, bukan dibiarkan lewat. Payload disimpan sebagai JSON
dengan nama field camelCase; field `version`, `id`, `createdAt`, dan `updatedAt` ada di setiap
record.

## Group

```json
{ "version": 1, "id": "…", "name": "Work", "createdAt": 1760000000, "updatedAt": 1760000000 }
```

- `name` wajib setelah trim dan maksimum 100 karakter.
- `name` unik tanpa memandang huruf besar/kecil, supaya daftar tidak pernah memuat dua nama yang
  terlihat sama.

## Host

```json
{
	"version": 1,
	"id": "…",
	"label": "web-1",
	"address": "web-1.example.com",
	"port": 22,
	"username": "root",
	"groupId": "… atau null",
	"credentialId": "… atau null",
	"createdAt": 1760000000,
	"updatedAt": 1760000000
}
```

- `label` (maks 100), `address` (maks 255), dan `username` (maks 100) wajib setelah trim.
- `port` harus berada pada rentang 1..65535.
- Tidak ada tags, icon, color, notes, jump host, forwarding, atau startup command pada fase ini.

## Credential

```json
{
	"version": 1,
	"id": "…",
	"name": "Prod",
	"secret": { "kind": "password", "password": "…" },
	"createdAt": 1760000000,
	"updatedAt": 1760000000
}
```

Jenis secret:

| `kind`        | Field                                  |
| ------------- | -------------------------------------- |
| `password`    | `password`                             |
| `private_key` | `private_key`, `passphrase` (opsional) |

- `name` wajib dan maksimum 100 karakter; secret wajib dan tidak boleh kosong; panjang secret
  dibatasi 16 KiB.
- Secret disimpan apa adanya tanpa trim, karena password dapat memuat spasi di ujungnya.
- Tidak ada SSH agent, certificate, hardware key, atau key path reference pada fase ini.

### Secret tidak pernah kembali ke frontend

Frontend hanya menerima `CredentialSummary`:

```text
CredentialSummary { id, name, kind, createdAt, updatedAt }
```

Tidak ada field secret pada tipe itu, sehingga secret tidak dapat dikirim kembali. Konsekuensinya:

- Mengganti nama credential tidak menyentuh secret-nya.
- Form edit tidak pernah memuat secret lama; secret hanya dapat dibuat atau diganti dengan nilai
  baru.
- Mengganti secret berarti mengirim nilai baru, bukan membaca yang lama.

## Relasi

Relasi disimpan sebagai id, bukan salinan data:

- `Host.groupId` menunjuk satu Group aktif, atau `null`.
- `Host.credentialId` menunjuk satu Credential aktif, atau `null`.

Relasi diperiksa di service layer **setelah** payload didekripsi, karena vault sendiri tidak dapat
melihat isi object:

- Host tidak dapat dibuat atau diubah agar menunjuk record yang tidak ada atau sudah dihapus.
- Group yang masih dipakai Host tidak dapat dihapus.
- Credential yang masih dipakai Host tidak dapat dihapus.

Tidak ada cascade delete: record yang masih dipakai harus dipindahkan atau dihapus lebih dulu.

## Delete

Semua delete memakai tombstone: row tetap ada dan `deleted_at` diisi. Object yang sudah dihapus
hilang dari daftar aktif dan tidak dapat dibaca lagi.

## Yang belum ada

Connecting dari saved Host, persistent known-host, auto-run credential, SSH agent, certificate,
tags, notes, search, import OpenSSH config, backup/export/import, auto-lock, dan sinkronisasi
berada di luar Phase 1B1.
