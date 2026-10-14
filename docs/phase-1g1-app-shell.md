# Dokumentasi Phase 1G-1: Desktop App Shell, Workspace Navigation, Sidebar, and Native Title Bar

Dokumen ini mencatat arsitektur desktop app shell yang diimplementasikan pada Termust Phase 1G-1, kepemilikan state antarmuka, aturan mounting workspace, integrasi title bar native Tauri, perbedaan lintas sistem operasi, daftar file yang berubah, batasan yang ditunda ke sub-fase berikutnya, serta checklist pengujian native manual.

---

## 1. Struktur AppShell yang Diimplementasikan

Phase 1G-1 menyusun tata letak desktop root full-window tanpa scroll global pada page/body (`overflow-hidden`). Layout terbagi menjadi tiga komponen utama di bawah `src/lib/components/app-shell/`:

```text
┌────────────────────────────────────────────────────────────────────────┐
│ AppTitlebar: Safe Margin (macOS) | Hosts Button | Session Tabs | Status│
├──────────────┬─────────────────────────────────────────────────────────┤
│ AppSidebar   │ Active Workspace (h-full w-full overflow-hidden)        │
│              │                                                         │
│ Hosts        │ - Inventory Workspace (overflow-y-auto, cards, forms)   │
│ Terminal     │   atau                                                  │
│              │ - Terminal Workspace (TerminalSessionPane, fit/resize)  │
│              │                                                         │
│ Vault status │                                                         │
└──────────────┴─────────────────────────────────────────────────────────┘
```

### Komponen Baru dan Tanggung Jawab

1. `src/lib/components/app-shell/app-shell.svelte`:
   - Menjadi kontainer root window `h-screen w-screen overflow-hidden flex flex-col`.
   - Menempatkan `titlebar`, `sidebar`, dan `children` (area kerja aktif).
   - Memastikan hanya workspace aktif yang mengelola scroll vertikal internal.
   - Bersih dari business logic, kredensial, command SSH, maupun global store.

2. `src/lib/components/app-shell/app-titlebar.svelte`:
   - Header terpadu dengan tinggi semantik `h-titlebar` (44px).
   - Menyediakan safe area 80px (`w-20`) di sisi kiri khusus traffic lights macOS.
   - Menampung navigasi cepat ke workspace Hosts (`Button`).
   - Mengintegrasikan komponen tab multi-sesi yang sudah ada (`SessionTabStrip`).
   - Menyediakan area kosong draggable (`data-tauri-drag-region`) hanya pada spacer non-interaktif.
   - Menampilkan status ringkas sesi aktif dan tombol Disconnect.
   - Tidak menyimpan kredensial atau state sesi sendiri.

3. `src/lib/components/app-shell/app-sidebar.svelte`:
   - Navigasi samping kiri dengan transisi collapse/expand yang menghormati `prefers-reduced-motion`.
   - Lebar saat terbuka memakai `w-sidebar` (240px); lebar saat collapse memakai `w-sidebar-collapsed` (56px).
   - Memiliki tombol toggle dengan `aria-expanded` dan icon `@lucide/svelte` (`PanelLeftClose`, `PanelLeftOpen`).
   - Item navigasi nyata: Hosts dan Terminal (disertai session count badge saat ada sesi aktif).
   - Menggunakan elemen `<nav aria-label="Main Navigation">` dan `aria-current="page"`.
   - Menampilkan status vault pasif di bagian bawah (unlocked, locked, atau uninitialized).
   - Mempertahankan accessible name pada tombol dengan kelas `sr-only` saat sidebar dalam kondisi collapsed.
   - Tidak menampilkan tombol navigasi palsu (tidak ada SFTP, Port Forwarding, Serial, Logs, Tags, atau Cloud Sync).

---

## 2. Ownership State Workspace dan Sidebar

State navigasi dimiliki langsung oleh root orchestrator `src/routes/+page.svelte`:

```ts
type Workspace = 'inventory' | 'terminal';

let activeWorkspace = $state<Workspace>('inventory');
let sidebarCollapsed = $state(false);
```

### Aturan Transisi Navigasi

1. Default saat aplikasi dibuka: `activeWorkspace = 'inventory'`.
2. Klik tombol Hosts pada sidebar atau titlebar: mengubah `activeWorkspace = 'inventory'`.
3. Klik tombol Terminal pada sidebar: mengubah `activeWorkspace = 'terminal'` (termasuk ketika belum ada sesi).
4. Memilih session tab pada titlebar:
   - memperbarui `activeSessionId`;
   - otomatis berpindah ke `activeWorkspace = 'terminal'`.
5. Submit form Quick Connect:
   - membuat sesi baru dan membuka tab;
   - otomatis berpindah ke `activeWorkspace = 'terminal'`.
6. Klik Connect dari saved host:
   - membuat sesi baru dan membuka tab;
   - otomatis berpindah ke `activeWorkspace = 'terminal'`.
7. Menutup tab terakhir:
   - tidak memaksa kembali ke inventory;
   - Terminal Workspace menampilkan empty state dengan tombol opsi membuka Hosts.
8. Collapse/expand sidebar:
   - membalik status `sidebarCollapsed`;
   - tidak mengubah `activeWorkspace` maupun `activeSessionId`.

---

## 3. Aturan Mounting Inventory dan Terminal

Lifecycle terminal Phase 1F mewajibkan seluruh instansi xterm.js dan PTY connection tetap terisolasi dan tidak di-unmount saat pengguna berpindah tab maupun berpindah workspace:

- **Terminal Workspace**:
  - Semua `TerminalSessionPane` yang terdaftar dalam `sessions` tetap terpasang dalam DOM.
  - Kontainer Terminal Workspace disembunyikan menggunakan kelas CSS `class:hidden={activeWorkspace !== 'terminal'}` bukan `{#if}`.
  - Nilai prop `active` untuk `TerminalSessionPane` dihitung dengan:
    ```svelte
    active={activeWorkspace === 'terminal' && session.id === activeSessionId}
    ```
  - Saat berada di Inventory Workspace, nilai `active` menjadi `false`, sehingga xterm tidak menangkap input keyboard.
  - Saat kembali ke Terminal Workspace, nilai `active` menjadi `true`, memicu `$effect` internal `TerminalSessionPane` untuk menjalankan `fit()` dan `focus()`.
  - Scrollback, session ID, runtime registry, dan event routing backend tetap utuh.

- **Inventory Workspace**:
  - Kontainer Inventory Workspace disembunyikan menggunakan kelas CSS `class:hidden={activeWorkspace !== 'inventory'}`.
  - State form Quick Connect, seleksi data, dan form modal tidak hilang ketika pengguna melihat terminal.

- **Global Overlays dan Alerts**:
  - `Dialog` verifikasi host key diletakkan di root komponen sehingga dialog tetap tampil di atas aplikasi saat dibutuhkan, terlepas dari workspace aktif.
  - Banner peringatan error global (`eventsError`) ditempatkan di root main container agar langsung terbaca pengguna.

---

## 4. Konfigurasi Titlebar Tauri

Konfigurasi window pada `src-tauri/tauri.conf.json` diperbarui:

```json
{
	"title": "Termust",
	"width": 1024,
	"height": 720,
	"minWidth": 640,
	"minHeight": 420,
	"resizable": true,
	"fullscreen": false,
	"decorations": true,
	"titleBarStyle": "Overlay",
	"hiddenTitle": true
}
```

### Detail Perilaku

- `titleBarStyle: "Overlay"` dan `hiddenTitle: true` mengaktifkan integrasi native traffic lights macOS ke dalam area header HTML aplikasi.
- `decorations: true` dipertahankan sehingga window controls asli sistem operasi tetap aktif.
- Window dragging ditangani secara deklaratif menggunakan atribut `data-tauri-drag-region` pada spacer kosong HTML titlebar.
- Atribut drag region TIDAK dipasang pada tombol navigasi, tab strip, tombol close tab, atau tombol disconnect. Klik dan seleksi pada kontrol interaktif tidak menyeret window.
- Tidak ada penambahan permission baru pada `src-tauri/capabilities/default.json` karena manipulasi window tidak memerlukan pemanggilan Window API secara manual.

---

## 5. Perbedaan Perilaku macOS dengan Windows dan Linux

| Aspek                | macOS                                                                                   | Windows dan Linux                                                                     |
| :------------------- | :-------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------ |
| **Window Frame**     | Titlebar menyatu (Overlay) dengan traffic lights native di kiri atas.                   | Native decorations tetap hadir di atas window (titlebar OS standar).                  |
| **Safe Margin Kiri** | Area kosong `w-20` (80px) melindungi traffic lights agar tidak menimpa tombol aplikasi. | Area `w-20` berfungsi sebagai area kosong draggable di titlebar HTML.                 |
| **Window Dragging**  | Window dapat diseret melalui area kosong titlebar aplikasi (`data-tauri-drag-region`).  | Window dapat diseret melalui native titlebar OS maupun area kosong titlebar aplikasi. |
| **Window Controls**  | Native traffic lights (Close, Minimize, Zoom) di kiri atas.                             | Native controls (Minimize, Maximize, Close) di kanan atas pada frame OS.              |

---

## 6. Daftar File yang Berubah

1. `src-tauri/tauri.conf.json`:
   - Menambahkan `"decorations": true`, `"titleBarStyle": "Overlay"`, `"hiddenTitle": true`.
2. `src/lib/components/session-tab-strip.svelte`:
   - Mengganti warna status dot hardcoded dengan design tokens semantik (`bg-status-idle`, `bg-status-disconnecting`, `bg-status-connected`, `bg-status-error`, `bg-status-connecting`).
   - Menerima prop opsional `class` untuk kustomisasi styling tanpa merusak fungsionalitas tab.
3. `src/lib/components/app-shell/app-shell.svelte` (baru):
   - Kontainer root layout desktop.
4. `src/lib/components/app-shell/app-titlebar.svelte` (baru):
   - Titlebar desktop dengan safe area macOS dan integrasi `SessionTabStrip`.
5. `src/lib/components/app-shell/app-sidebar.svelte` (baru):
   - Sidebar navigasi collapsible dengan indikator status vault pasif.
6. `src/routes/+page.svelte`:
   - Refactor root page orchestrator untuk mengintegrasikan `AppShell`, memisahkan workspace, dan mengatur mounting yang aman.
7. `docs/phase-1g1-app-shell.md` (baru):
   - Dokumentasi arsitektur dan checklist pengujian manual.

---

## 7. Batas yang Sengaja Ditunda ke Phase 1G-2 dan Phase 1G-3

- **Phase 1G-2 (Inventory Workspace)**:
  - Pemecahan komponen `inventory-panel.svelte` menjadi `host-workspace`, `host-grid`, `host-card`, dan `host-detail-panel`.
  - Context panel kanan untuk detail host.
- **Phase 1G-3 (Terminal Workspace)**:
  - Pemindahan terminal tools (snippet runner) ke context panel kanan di Terminal Workspace.
  - Pemisahan `terminal-workspace.svelte`.

---

## 8. Manual Native Verification Checklist

Checklist ini wajib diverifikasi oleh manusia pada aplikasi native yang berjalan:

```text
- [ ] Aplikasi terbuka tanpa white screen.
- [ ] Traffic lights macOS tampil dan dapat digunakan.
- [ ] Konten titlebar tidak tertutup traffic lights.
- [ ] Window dapat diseret dari area kosong titlebar.
- [ ] Klik workspace button tidak menyeret window.
- [ ] Klik session tab tidak menyeret window.
- [ ] Close session tab tetap bekerja.
- [ ] Sidebar dapat collapse dan expand.
- [ ] Hosts membuka Inventory Workspace.
- [ ] Terminal membuka Terminal Workspace.
- [ ] Quick Connect otomatis membuka Terminal Workspace.
- [ ] Saved Host Connect otomatis membuka Terminal Workspace.
- [ ] Berpindah Inventory ↔ Terminal tidak menghapus scrollback.
- [ ] Berpindah workspace tidak membuat sesi SSH baru.
- [ ] Beberapa session tabs tetap independen.
- [ ] Host-key dialog tetap muncul untuk session yang benar.
- [ ] Terminal kembali fit setelah workspace ditampilkan.
- [ ] Resize window pada ukuran minimum tidak memotong kontrol penting.
- [ ] Windows/Linux tetap menggunakan native decorations.
```
