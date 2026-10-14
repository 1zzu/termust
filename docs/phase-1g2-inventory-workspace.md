# Dokumentasi Phase 1G-2: Inventory Host Workspace, Host Grid, and Detail Panel

Dokumen ini mencatat pembaruan antarmuka bagian Hosts pada Termust Phase 1G-2, pembagian kepemilikan komponen, alur data, perilaku pencarian dan filter, perilaku seleksi dan panel detail, batas keamanan, pekerjaan yang ditunda, serta checklist pengujian manual.

---

## 1. Component Ownership

Komponen presentasi bagian Hosts dipecah menjadi unit-unit terfokus di bawah `src/lib/components/inventory/`:

1. `src/lib/components/inventory/host-workspace.svelte`:
   - Bertindak sebagai layout utama section Hosts di dalam inventory workspace.
   - Mengelola state UI lokal: `searchQuery`, `selectedGroupId`, dan `selectedHostId`.
   - Menghasilkan filtered host list secara lokal menggunakan pure helper `filterHosts`.
   - Mengatur penataan toolbar, empty state, grid host, dan context detail panel.
   - Tidak melakukan pemanggilan IPC atau Tauri secara langsung.

2. `src/lib/components/inventory/host-grid.svelte`:
   - Menerima daftar host yang sudah difilter (`hosts: Host[]`).
   - Merender kumpulan `HostCard` dalam responsive grid.
   - Menyediakan empty state terpisah untuk inventory kosong vs hasil pencarian/filter kosong.
   - Tidak menyimpan salinan independen dari daftar host.

3. `src/lib/components/inventory/host-card.svelte`:
   - Menampilkan label host, `username@address:port`, nama group, nama credential, dan petunjuk kebutuhan credential.
   - Menampilkan status terpilih melalui semantic ring dan border (`ring-1 ring-primary border-primary bg-accent/40`).
   - Menyediakan aksi Connect mandiri dan selectable area untuk memilih host.
   - Menghindari nested interactive element (area pemilihan dan tombol aksi Connect diletakkan terpisah sebagai sibling element).
   - Tidak memuat atau menampilkan credential secret.

4. `src/lib/components/inventory/host-detail-panel.svelte`:
   - Ditampilkan hanya saat ada host yang dipilih.
   - Menampilkan field riil: label, address, port, username, group, nama credential, tipe credential, serta timestamp createdAt dan updatedAt.
   - Menyediakan aksi Connect, Edit, Delete, dan Close detail panel.
   - Menggunakan token lebar desktop `w-context-panel` (320px).
   - Menutup panel saat tombol Escape ditekan melalui window listener.
   - Tidak menampilkan metrik sintetis (uptime, latency, last connected, logs, atau tags).

5. `src/lib/components/inventory/host-filter.ts`:
   - Pure helper TypeScript tanpa side-effects untuk menyaring host secara case-insensitive berdasarkan label, address, atau username, serta filter group atau ungrouped.

6. `src/lib/components/inventory-panel.svelte`:
   - Tetap menjadi pengontrol utama data decrypted dan operasi CRUD.
   - Mengelola dialog pembuatan dan pengeditan host, group, credentials, known hosts, snippets, dan backup.
   - Meneruskan data decrypted ke `HostWorkspace` melalui typed props dan menerima aksi melalui typed callbacks.
   - Membersihkan seluruh state inventory saat vault terkunci.

---

## 2. Data Flow

```text
inventory-panel.svelte (Controller State Decrypted)
  │
  ├── props: hosts, groups, credentials, busy, vaultStatus, hostConnecting
  │   callbacks: onconnect, onedit, ondelete, oncreate, onimported
  ▼
host-workspace.svelte (Local UI State: search, filter, selection)
  │
  ├── filterHosts(hosts, { query, groupId }) ──► filteredHosts
  │
  ├── HostGrid (Render kartu & empty state)
  │     └── HostCard (Informasi host, status ring, tombol Connect)
  │
  └── HostDetailPanel (Ditampilkan jika selectedHost !== null)
        └── Aksi Connect, Edit, Delete, Close
```

---

## 3. Search dan Filter Behavior

- Pencarian lokal bersifat case-insensitive dan membandingkan query dengan `label`, `address`, dan `username`.
- Filter group menyediakan opsi:
  - `All groups`: menampilkan seluruh host yang cocok dengan search query.
  - `Ungrouped`: hanya menampilkan host tanpa group (`groupId === null` atau kosong).
  - Pilihan nama group spesifik: menyaring host berdasarkan UUID group terkait.
- Jika group yang sedang dipilih dihapus dari vault, filter otomatis kembali ke `All groups`.
- Pencarian dan filter dieksekusi murni di frontend tanpa IPC request baru ke backend.

---

## 4. Selected Host dan Detail Panel Behavior

- Default: tidak ada host yang terpilih (`selectedHostId = null`).
- Klik pada area utama `HostCard` memilih host dan membuka `HostDetailPanel`.
- Memilih host lain langsung memperbarui konten `HostDetailPanel`.
- Tombol Close pada header detail panel atau penekanan tombol Escape menghapus seleksi.
- Konsistensi filter dan seleksi: saat search query atau group filter menyembunyikan host yang sedang dipilih dari hasil filter, seleksi otomatis dibersihkan (`selectedHostId = null`) dan panel detail ditutup.
- Setelah aksi Delete atau refresh yang menyebabkan host terpilih tidak lagi ada di inventory, seleksi otomatis dibersihkan.
- Layout desktop: detail panel ditempatkan di sebelah kanan grid menggunakan lebar token semantik `w-context-panel`. Pada window sempit, panel turun ke bawah grid secara responsif tanpa overflow horizontal.

---

## 5. Security Invariants

- Komponen host tidak memiliki akses ke credential secrets (password, private key, passphrase).
- Objek `CredentialSummary` hanya memuat metadata publik (`id`, `name`, `kind`, `createdAt`, `updatedAt`).
- Host components tidak memanggil Tauri IPC secara langsung. Aksi connect saved host tetap hanya meneruskan `hostId` ke root orchestrator via alur existing.
- Saat vault terkunci (`vaultStatus !== 'unlocked'`), seluruh decrypted list dibersihkan oleh `inventory-panel.svelte` dan detail panel tertutup.
- Tidak ada data inventory yang disimpan di `localStorage` atau `sessionStorage`.

---

## 6. Pekerjaan yang Ditunda

- **Phase 1G-3 (Terminal Workspace)**: Snippet runner tools panel di sebelah kanan terminal workspace, status bar koneksi terminal.
- **Phase 1G-4 (Forms dan Dialogs)**: Redesign komprehensif modal form Quick Connect, Host Create/Edit dialog, dan Credential dialog.
- **Phase 1G-5 (Responsive dan Mobile)**: Drawer sheet penuh untuk navigasi mobile dan overlay sheet context panel di viewport tablet/mobile.

---

## 7. Manual Smoke-Test Checklist

- [ ] Toolbar Hosts tampil rapi dengan jumlah host yang akurat.
- [ ] Pencarian case-insensitive bekerja untuk label, address, dan username.
- [ ] Filter group bekerja untuk All groups, Ungrouped, dan grup tertentu.
- [ ] Pemilihan kartu host melalui klik mouse menampilkan detail panel di sebelah kanan.
- [ ] Navigasi keyboard (Tab, Space, Enter) dapat memilih host card dan menekan tombol Connect.
- [ ] Tombol Connect pada card dan detail panel aktif jika host memiliki credential, dan disabled jika belum memiliki credential ("Credential required").
- [ ] Tombol Connect membuka sesi terminal baru ke host yang dituju.
- [ ] Tombol Edit pada detail panel membuka dialog edit host dengan data yang tepat.
- [ ] Tombol Delete pada detail panel menghapus host dan membersihkan seleksi.
- [ ] Tombol Close atau tombol Escape menutup panel detail.
- [ ] Tombol New Host membuka dialog pembuatan host baru.
- [ ] Tombol Import OpenSSH membuka dialog import wizard dan memicu refresh setelah import berhasil.
- [ ] Mengunci vault membersihkan daftar host dan menutup panel detail seketika.
- [ ] Window resizing tidak menimbulkan scrollbar horizontal pada outer page.

---

## 8. Phase 1G-2R1 Refinements: Refined Hosts Workspace and Unified Context Panel

Revisi ini menyempurnakan hasil Phase 1G-2 berdasarkan evaluasi visual native desktop:

### A. Penghapusan Card Wrapper Saved Inventory dan Toolbar Seamless

- Menghilangkan Card wrapper besar, heading, dan deskripsi "Saved inventory" dari tampilan Hosts.
- Section selector untuk Hosts, Credentials, Groups, Known Hosts, Snippets, dan Backup diubah menjadi navigation row yang ringkas dan menyatu langsung dengan workspace.
- Toolbar pada `host-workspace.svelte` diletakkan langsung di bagian atas workspace tanpa outer card container, background berlebih, atau padding bertingkat.
- Search input menjadi kontrol utama dengan alokasi ruang fleksibel terbesar, berdampingan dengan host count, group filter, tombol Quick Connect, Import OpenSSH, dan New Host.

### B. Penyederhanaan Host Card

- `host-card.svelte` dirampingkan untuk hanya menampilkan nama atau label host, badge group jika ada, badge credential jika terpasang, atau badge peringatan "Credential required" bila credential belum dihubungkan.
- Menghapus string `username@address:port`, timestamp, teks "Selected", footer card, dan tombol Connect langsung dari kartu.
- Detail teknis koneksi dan timestamp dipusatkan pada Host Detail Panel.
- Status terpilih ditampilkan secara visual dan semantik menggunakan ring, border, latar aksen, serta atribut `aria-pressed="true"`.

### C. Single-Click Selection dan Double-Click Connect

- **Single click**: Memilih host, memperbarui state context panel, dan membuka Host Detail Panel tanpa memulai koneksi.
- **Double click**: Menggunakan event `ondblclick` asli browser. Jika host memenuhi syarat connectable (memiliki credential, backend tidak sibuk, dan tidak sedang dalam status menghubungkan), double-click langsung memicu callback `onconnect(host)`. Jika credential belum terpasang, double-click tidak mengirimkan request ke backend dan hanya membuka detail panel.
- **Keyboard navigation**: Card dapat difokuskan via Tab dan dipilih menggunakan tombol Enter atau Space. Pengguna keyboard mengeksekusi koneksi melalui tombol Connect utama di Host Detail Panel.

### D. Unified Context Panel State

- Menghindari beberapa boolean flag yang dapat menyebabkan inkonsistensi UI. State context panel didefinisikan sebagai discriminated union:
  ```ts
  type InventoryContext = { kind: 'host'; hostId: string } | { kind: 'quick-connect' } | null;
  ```
- **Ownership State**: Dikelola secara lokal di `host-workspace.svelte` sebagai boundary terdekat yang mengontrol toolbar, grid host, detail panel host, dan Quick Connect panel tanpa membutuhkan global store.
- **Invariants Context**:
  - Default: `null`.
  - Memilih host membuka context `host` dan menutup Quick Connect jika sebelumnya aktif.
  - Menekan tombol Quick Connect membuka context `quick-connect` dan membersihkan seleksi host.
  - Host Detail Panel dan Quick Connect Panel tidak pernah dirender bersamaan.
  - Menutup panel atau menekan tombol Escape mengembalikan state ke `null`.
  - Jika filter atau pencarian membuat host terpilih tidak terlihat, context `host` otomatis direset ke `null`.

### E. Quick Connect Right Context Panel dan Security Ownership

- Card Quick Connect di bagian bawah halaman pada `+page.svelte` dihapus dan dipindahkan ke right context panel (`quick-connect-panel.svelte`) dengan token lebar desktop `w-context-panel`.
- **Session Lifecycle & Invariants**: Seluruh form state, validation, dan lifecycle orchestration tetap dimiliki oleh `+page.svelte` dan diteruskan melalui binding/callback. Submit form tetap membuat `TerminalSession` baru, mengalihkan `activeWorkspace` ke `terminal`, dan memanggil `lifecycle.begin(...)`.
- **Secret Hygiene**: Credential (password, private key, passphrase) hanya disimpan dalam memori form komponen root. Saat Quick Connect panel ditutup tanpa submit (atau saat pengguna berpindah ke host lain atau section lain), callback `onquickclear` dipanggil untuk mengosongkan seluruh field rahasia. Nilai non-secret (host, port, username, auth method) tetap dipertahankan untuk kenyamanan pengguna.
- Tidak ada rahasia yang diteruskan ke HostCard, HostGrid, context state helper, atau disimpan ke persistent storage.

### F. Hal yang Ditunda ke Phase 1G-2R2

- Vault gate dan blur overlay.
- Pemindahan tombol Lock ke sidebar atau penghapusan `VaultPanel`.
- Posisi dan layout tombol traffic lights macOS.
- Active-state styling dan tab indicator pada terminal tab strip.
