# Kontrak UI dan Fondasi Desain (Phase 1G)

Dokumen ini menetapkan kontrak arsitektur antarmuka, pembagian kepemilikan komponen (ownership), matriks status, invariant terminal Phase 1F, panduan responsif, aksesibilitas, design tokens, dan rencana migrasi bertahap untuk perombakan UI Termust pada Phase 1G.

Phase 1G-0 adalah penyiapan fondasi dan kontrak arsitektur. Phase ini tidak melakukan perombakan layout visual secara besar-besaran, tidak mengubah backend Rust, tidak mengubah konfigurasi window Tauri, dan tidak mengubah lifecycle SSH.

---

## 1. Audit UI Saat Ini (Baseline Phase 1F)

Audit ini mencatat struktur kode aktual yang ada pada repositori per commit `f9233a0b25237df717d925554944cd43d27154a0`:

### A. Komponen yang Terlalu Besar

1. `src/lib/components/inventory-panel.svelte` (1.190 baris):
   - Menggabungkan terlalu banyak domain: CRUD Host, CRUD Credential, CRUD Group, Known Hosts, Snippets (CRUD dan eksekusi preview), tombol modal Backup/Restore, dan modal Import OpenSSH.
   - Mengelola state form lokal untuk masing-masing entity di satu file raksasa.
   - Sulit dirawat dan diuji secara terpisah karena logika tampilan host menyatu dengan form modal dan daftar snippet.
2. `src/routes/+page.svelte` (517 baris):
   - Bertindak sebagai root orchestrator sekaligus merender seluruh form Quick Connect, status bar header, scrollable cards container, tab strip, terminal pane loop, dan dialog verifikasi host key.
3. `src/lib/components/backup-restore.svelte` (493 baris):
   - Mengelola wizard export dan inspect/restore bertahap.
4. `src/lib/components/import-openssh-dialog.svelte` (383 baris):
   - Mengelola wizard import bertahap dengan preview dan seleksi baris.

### B. State yang Dimiliki `+page.svelte`

- `vaultStatus` (`VaultStatus | undefined`): status vault (uninitialized, locked, unlocked).
- Form Quick Connect: `host`, `port`, `username`, `auth`, `password`, `privateKey`, `passphrase`, `quickSubmitting`.
- Tab & Sesi: `sessions: TerminalSession[]` (daftar seluruh tab yang terbuka).
- `activeSessionId: string | null` (id tab yang sedang aktif).
- `eventsError: string` (error jika backend event listener gagal terpasang).
- Derived state:
  - `activeSession` (objek session yang sedang aktif).
  - `anyLiveSession` (boolean apakah ada sesi live atau stopping).
  - `canDisconnect` (apakah tab aktif dapat diputus).
  - `activePrompt` (host key prompt milik sesi aktif).
- Unreactive registry: `runtimes: Record<string, TerminalRuntime>` (mapping runtime xterm per tab).
- Orchestrator: `lifecycle` (`createSessionLifecycle(...)`).

### C. Action dan Callback Penting

- `handleSubmit(event)`: memvalidasi form Quick Connect, membuat sesi baru via `createSession`, membuka tab, dan memanggil `lifecycle.begin(...)`.
- `connectSavedHost(savedHost)`: membuka sesi dari saved host (hanya mengirim hostId ke backend) dan memanggil `lifecycle.begin(...)`.
- `openTab(session)`: menambahkan session descriptor ke array `sessions` dan mengaktifkannya.
- `selectTab(sessionId)`: mengubah `activeSessionId` tanpa mengubah state koneksi atau menghapus scrollback.
- `handleReady(sessionId, runtime)`: mendaftarkan instance `TerminalRuntime` ke registry dan memberi tahu `lifecycle.terminalReady(sessionId)`.
- `closeTab(sessionId)`: memanggil `lifecycle.close(sessionId)` dan membersihkan runtime setelah tab benar-benar ditutup.
- `answerHostKey(accepted)`: merespons verifikasi host key untuk `prompt.sessionId` terkait (bukan active session id), kemudian memindahkan fokus ke sesi lain yang masih menunggu prompt jika ada.
- `clearCredentials()`: membersihkan password, private key, dan passphrase dari memori komponen setelah submit atau saat pembatalan.

### D. Komponen yang Sudah Reusable

- `src/lib/components/session-tab-strip.svelte`: murni presentasional dan menangani keyboard navigation tab list.
- `src/lib/components/terminal-session.svelte`: mengisolasi xterm.js instance, FitAddon, ResizeObserver, dan PTY I/O per sesi.
- `src/lib/components/backup-restore.svelte`: mengisolasi alur backup dan restore terenkripsi.
- `src/lib/components/import-openssh-dialog.svelte`: mengisolasi wizard import config OpenSSH.
- Primitif shadcn-svelte di `src/lib/components/ui/` (`alert`, `button`, `card`, `checkbox`, `dialog`, `input`, `label`, `select`, `separator`, `textarea`).

### E. Komponen yang Terlalu Terikat pada Halaman

- Form Quick Connect: menyatu langsung dalam card di `+page.svelte`.
- Card container dan layout root: menggunakan scroll vertikal halaman yang mengapit tab strip dan terminal.

### F. Bagian yang Aman Dipindahkan pada Fase Berikutnya

- Pemisahan layout container menjadi `AppShell`, `AppTitlebar`, dan `AppSidebar`.
- Pemecahan `inventory-panel.svelte` menjadi `host-workspace.svelte`, `host-grid.svelte`, `host-card.svelte`, dan `host-detail-panel.svelte`.
- Pemisahan Quick Connect ke panel/dialog tersendiri.
- Pemindahan drawer snippet dan terminal tools ke `ContextPanel` di kanan.

### G. Bagian yang TIDAK BOLEH Dipindahkan Sembarangan (Terkait Lifecycle)

- State `sessions` dan `activeSessionId` harus tetap memiliki satu sumber kebenaran (single source of truth).
- `createSessionLifecycle`: keterikatan listener Tauri fail-closed dan antrean start cancellable.
- Container tab `TerminalSessionPane`: setiap tab HARUS tetap ter-mount di DOM (`hidden={!active}`), tidak boleh di-unmount dengan `{#if}` saat berpindah tab agar xterm instance dan scrollback tidak hilang.
- Pembersihan kredensial Quick Connect via callback `onQuickStartFinished`.
- Penanganan host-key prompt yang terikat pada ID sesi pemilik prompt.
- Perhitungan `hasLiveSessions` yang melindungi aksi restore database.

### H. Dependency shadcn-svelte yang Tersedia dan Rencana Kebutuhan

- Tersedia saat ini: `alert`, `button`, `card`, `checkbox`, `dialog`, `input`, `label`, `select`, `separator`, `textarea`.
- Kandidat komponen shadcn-svelte untuk sub-phase berikutnya:
  - `tooltip`: untuk accessible icon-only button pada titlebar, sidebar, dan panel tools.
  - `sheet`: untuk mobile/tablet navigation drawer dan panel context overlay.
  - `dropdown-menu`: untuk menu opsi host card dan context action tab.
  - `badge`: untuk status badge dan indikator tag.
  - `collapsible`: untuk collapsible section pada sidebar dan context panel.
  - `scroll-area`: untuk viewport scrollable yang konsisten lintas platform desktop.

---

## 2. Tujuan Phase 1G

1. **Pola Layout Terminal Modern**: Mengadopsi struktur aplikasi desktop modern dengan app titlebar terintegrasi, sidebar navigasi, workspace utama, dan context panel yang collapsible.
2. **Identitas Visual Termust**: Desain gelap berbasis terminal (dark terminal-first), bersih, efisien, local-first, dan menghargai privasi pengguna.
3. **Bukan Kloning Produk Lain**: Termust memiliki identitas visual sendiri, tidak menyalin branding, skema warna, ikon, atau aset Termius.
4. **Kejujuran Antarmuka (C-2, R-24, R-26)**: Tidak menampilkan tombol aktif atau navigasi untuk fitur yang belum diimplementasikan (seperti SFTP, Port Forwarding, Serial, Sharing, Sync Cloud, atau Tags). Fitur yang belum ada tidak boleh memiliki kontrol palsu.

---

## 3. Struktur Aplikasi yang Dituju

Secara konseptual, hierarki aplikasi dirancang sebagai berikut:

```text
AppShell
├── AppTitlebar
│   ├── WorkspaceNavigation (Inventory vs Terminal)
│   └── SessionTabStrip (Daftar tab sesi aktif)
├── AppSidebar (Navigasi modul: Hosts, Keys, Snippets, Settings/Vault)
└── Workspace (Area kerja aktif sesuai konteks)
    ├── PrimaryContent (Daftar Host / Grid / Terminal Aktif)
    └── ContextPanel (Detail Host / Terminal Quick Tools / Snippet Runner)
```

### Tanggung Jawab Bagian

1. **AppShell**:
   - Kontainer root window yang mengatur tata letak 3 kolom (Sidebar, PrimaryContent, ContextPanel) dan header titlebar.
   - Mengelola state global tampilan: workspace aktif (`inventory` atau `terminal`), status collapse sidebar, dan visibilitas context panel.
2. **AppTitlebar**:
   - Area atas window yang menyatu dengan kontrol window native (pada platform yang mendukung).
   - Menampung `WorkspaceNavigation` untuk berpindah cepat antara mode Inventory dan Terminal.
   - Menampung `SessionTabStrip` horizontal saat dalam mode terminal atau saat ada sesi live.
3. **AppSidebar**:
   - Navigasi vertikal di sisi kiri.
   - Mode desktop: sidebar lebar 240px (dapat di-collapse menjadi rail 56px).
   - Menampilkan item modul: Saved Hosts, Groups, Credentials/Keys, Snippets, dan Vault Status.
4. **Workspace**:
   - **Inventory Workspace**:
     - _PrimaryContent_: Grid atau list Host yang tersimpan, filter grup, pencarian, dan tombol aksi tambah.
     - _ContextPanel_: Panel samping kanan (lebar 320px) yang menampilkan informasi host terpilih yang benar-benar tersedia: label host, hostname/address, port, username, relasi group jika tersedia, ringkasan credential yang aman tanpa secret, serta aksi Connect, Edit, Delete, dan tombol close/collapse panel. Tidak menambahkan riwayat koneksi, last connected, uptime, latency, logs, tags, sharing, atau data sintetis.
   - **Terminal Workspace**:
     - _PrimaryContent_: Area terminal xterm.js aktif (seluruh tab yang terpasang disembunyikan kecuali yang aktif).
     - _ContextPanel_: Panel terminal tools kanan (lebar 288px) yang hanya menggunakan state dan aksi yang tersedia: nama/target sesi, connection status, status message atau error yang memang sudah ada, snippet quick-run, tombol Disconnect, dan tombol close/collapse panel. Tidak menambahkan latency, network statistics, command history, autocomplete, atau telemetry palsu.

---

## 4. Component Ownership (Kepemilikan Komponen & State)

Prinsip utama: **Satu sumber kebenaran (single source of truth)**. Jangan membuat global store atau abstraction layer baru jika state lokal dan props callback sudah mencukupi.

| State / Objek                               | Pemilik Utama               | Alasan Kepemilikan                                                                                           |
| :------------------------------------------ | :-------------------------- | :----------------------------------------------------------------------------------------------------------- |
| `sessions: TerminalSession[]`               | `AppShell` / Orchestrator   | Seluruh window harus mengetahui daftar sesi untuk tab strip, status bar, restore guard, dan window teardown. |
| `activeSessionId: string \| null`           | `AppShell` / Orchestrator   | Mengontrol tab mana yang tampil, terminal mana yang difokuskan, dan target snippet aktif.                    |
| Instance `xterm.js` (`Terminal`)            | `terminal-session.svelte`   | Instance xterm terikat erat pada lifecycle container DOM dan ResizeObserver per tab.                         |
| `runtimes: Record<string, TerminalRuntime>` | `AppShell` / Orchestrator   | Registry unreactive untuk meneruskan output backend (`writeOutput`) dan trigger resize/fit ke tab tertentu.  |
| Seleksi Inventory (`selectedHostId`)        | `InventoryWorkspace`        | Detail panel host hanya relevan di dalam inventory workspace.                                                |
| Status Context Panel (Open / Closed)        | `Workspace` / `AppShell`    | Menentukan alokasi lebar grid antara PrimaryContent dan ContextPanel.                                        |
| State Form Credential / Host                | Dialog / Form masing-masing | Rahasia dan kredensial baru hanya hidup di dalam form selama input berlangsung, lalu dibuang.                |
| Vault Status (`vaultStatus`)                | Root Orchestrator           | Mempengaruhi izin seluruh workspace inventory dan proteksi master password.                                  |

### Alur Komunikasi Child ke Parent

Semua komunikasi dari anak ke induk wajib menggunakan typed callback props Svelte 5:

- `onselect(sessionId: string)`: saat tab diklik atau dipilih via keyboard.
- `onclose(sessionId: string)`: saat tombol close pada tab ditekan.
- `onconnect(host: Host)`: saat tombol Connect pada host ditekan.
- `onready(sessionId: string, runtime: TerminalRuntime)`: saat xterm container selesai di-mount.
- `onchange(...)`: saat filter atau seleksi inventory berubah.

---

## 5. Screen and State Matrix

| State Aplikasi                 | Tampilan Visual                                                                                      | Aksi yang Tersedia                                                                                                                                                                                             |
| :----------------------------- | :--------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Inventory tanpa host**       | Layar empty state dengan penjelasan dan panduan singkat.                                             | Tombol "Add Host", "Import OpenSSH", "Create Group".                                                                                                                                                           |
| **Inventory dengan host**      | Grid atau list host card dengan label, target IP/host, dan status credential.                        | Klik host untuk memilih, tombol "Connect", menu edit/delete, filter group.                                                                                                                                     |
| **Host terpilih**              | Host card aktif memiliki border ring; ContextPanel kanan menampilkan detail lengkap.                 | Tombol "Connect", "Edit Host", "Delete Host", tutup panel detail.                                                                                                                                              |
| **Vault locked**               | Banner peringatan vault terkunci; daftar inventory disembunyikan; prompt master password tampil.     | Masukkan master password untuk unlock; Quick Connect tetap dapat digunakan.                                                                                                                                    |
| **Vault unlocked**             | Seluruh data inventory, group, credentials, dan snippet dapat diakses penuh.                         | Tombol "Lock Vault", CRUD lengkap untuk semua entity vault.                                                                                                                                                    |
| **Tidak ada terminal session** | Terminal workspace menampilkan empty state terpusat.                                                 | Tombol "Quick Connect" dan shortcut membuka Saved Hosts.                                                                                                                                                       |
| **Session connecting**         | Tab strip menampilkan indikator status kuning (amber); terminal menampilkan cursor blink.            | Tombol "Close Tab" (membatalkan koneksi yang sedang berjalan).                                                                                                                                                 |
| **Session connected**          | Indikator status hijau (emerald); terminal aktif menerima input interaktif; tools panel aktif.       | Input keyboard terminal, resize, tombol "Disconnect", tombol "Close Tab", eksekusi snippet.                                                                                                                    |
| **Session disconnecting**      | Indikator status kuning; input terminal dinonaktifkan; tombol disconnect disabled.                   | Menunggu proses penghentian backend selesai.                                                                                                                                                                   |
| **Session disconnected**       | Indikator status abu-abu (muted); scrollback tetap utuh dan dapat dibaca/di-copy; input disabled.    | Tombol "Close Tab", scroll riwayat output, seleksi teks.                                                                                                                                                       |
| **Session error**              | Indikator status merah (rose); banner error menampilkan pesan kegagalan yang dapat ditindaklanjuti.  | Pengguna dapat membaca atau menyalin pesan error, menutup tab via tombol "Close Tab", atau mencoba kembali melalui form Quick Connect atau saved host (credential tidak dapat diedit langsung dari tab error). |
| **Host-key confirmation**      | Modal dialog verifikasi muncul di atas tab terkait dengan fingerprint SHA-256.                       | Tombol "Accept and connect", tombol "Cancel" (menolak dan memutus koneksi).                                                                                                                                    |
| **Banyak session tabs**        | Tab strip dapat di-scroll horizontal tanpa menambah tinggi window; tombol tab tetap terbaca.         | Navigasi keyboard (ArrowLeft, ArrowRight, Home, End), klik tab, tutup tab.                                                                                                                                     |
| **Sidebar collapsed**          | Sidebar menyusut menjadi icon rail (lebar 56px); label teks disembunyikan dengan tooltip accessible. | Klik icon untuk navigasi, tombol toggle expand/collapse.                                                                                                                                                       |
| **Context panel collapsed**    | Context panel kanan tertutup; PrimaryContent mengambil lebar penuh workspace.                        | Tombol toggle expand panel di header workspace.                                                                                                                                                                |

---

## 6. Responsive Contract

Responsif di Termust dirancang berdasarkan perilaku antarmuka (behavior-driven) menggunakan breakpoint standar Tailwind CSS:

- **Desktop (`lg: >= 1024px`)**:
  - Tata letak 3 kolom: Sidebar kiri (240px), Primary Workspace (fleksibel, min 480px), Context Panel kanan (288px–320px).
  - Tab sesi terminal horizontal di bagian atas.
  - Sidebar dan Context Panel dapat di-collapse secara mandiri.
  - Alokasi ruang terminal diprioritaskan agar lebar kolom terminal mencukupi minimal 80 kolom karakter monospace.
- **Tablet (`md: 768px - 1023px`)**:
  - Sidebar kiri otomatis berubah menjadi icon rail ringkas (lebar 56px) atau drawer geser.
  - Context Panel berubah menjadi sheet overlay atau slide-over drawer di atas konten utama saat diaktifkan.
  - Primary Workspace tetap dominan untuk kenyamanan mengetik dan membaca data.
- **Mobile (`< 768px`, batas minimal `sm: < 640px`)**:
  - Navigasi berpindah sepenuhnya ke hamburger menu / navigation sheet.
  - Host list ditampilkan dalam satu kolom vertikal.
  - Detail host dan form input tampil sebagai full-screen sheet.
  - Terminal mengambil ruang penuh (fullscreen mode) dengan switcher tab yang kompak di bar atas.
  - Terminal tools (snippet runner) dibuka melalui bottom sheet.
  - Semua target sentuh (touch target) wajib berukuran minimal 44x44px sesuai aturan aksesibilitas (R-03).
  - Memperhitungkan safe-area-inset pada perangkat mobile.

---

## 7. Terminal Invariants (Lifecycle Phase 1F)

Revamp UI pada Phase 1G tidak boleh merusak 11 invariant lifecycle terminal yang telah diverifikasi pada Phase 1F:

1. **Identitas Berbasis `sessionId`**: Identitas sesi SSH ditentukan secara tunggal oleh UUID `sessionId`, bukan oleh judul tab, hostname, atau urutan visual tab.
2. **Lifecycle Mandiri per Tab**: Setiap tab mengelola state koneksi dan instansinya sendiri. Kegagalan atau pemutusan satu tab tidak boleh mempengaruhi tab lain.
3. **Instance xterm Tidak Dibuat Ulang**: Saat pengguna berpindah tab, terminal yang tidak aktif hanya disembunyikan (`hidden`), tidak pernah di-unmount atau di-dispose.
4. **Scrollback Bertahan**: Riwayat terminal (scrollback) wajib tetap utuh saat berpindah tab maupun saat status sesi menjadi `disconnected`.
5. **No Unmount saat Hidden**: Tab tersembunyi tidak boleh di-render ulang dari awal. `fit()` tidak boleh dipanggil pada kontainer dengan dimensi nol.
6. **Cancel saat Connecting**: Menutup tab yang sedang berstatus connecting harus membatalkan start request, menunggu penyelesaian start (`start.done`), lalu memanggil disconnect di backend sebelum menghapus tab.
7. **Isolasi Disconnect**: Aksi Disconnect hanya menghentikan sesi aktif yang dituju. Tab tetap dipertahankan beserta outputnya.
8. **Routing Backend Tepat Sasaran**: Semua event backend (`ssh://data`, `ssh://status`, `ssh://error`, `ssh://host-key`) dirutekan semata-mata berdasarkan `sessionId` payload. Output sesi A tidak pernah bocor ke terminal B.
9. **Kredensial Bebas dari Descriptor**: Objek `TerminalSession` tidak boleh memuat password, private key, passphrase, atau credential secret.
10. **Proteksi Restore Database**: Perintah restore backup wajib ditolak selama masih ada minimal satu sesi yang live, connecting, atau stopping (`hasLiveSessions`).
11. **Fail-Closed Event Listeners**: Backend SSH tidak boleh dipanggil sebelum seluruh 4 listener event Tauri siap dan terminal tab telah selesai di-mount.

---

## 8. Accessibility Contract

Antarmuka Termust wajib memenuhi standar aksesibilitas keyboard dan visual:

1. **Keyboard Navigation Penuh**: Seluruh tombol, tab, form control, dan item list dapat diakses via keyboard (`Tab`, `Shift+Tab`). Tab strip menggunakan pola roving tabindex dengan tombol `ArrowLeft`, `ArrowRight`, `Home`, dan `End`.
2. **Visible Focus Indicator**: Semua elemen yang fokus wajib memiliki ring fokus yang terlihat jelas (`focus-visible:ring-2`). Larangan menghapus outline tanpa indikator pengganti (R-32).
3. **Accessible Icon-Only Buttons**: Setiap tombol yang hanya menampilkan ikon wajib memiliki atribut `aria-label` dan tooltip yang dapat dibaca screen reader.
4. **Kontras Warna WCAG AA**:
   - Teks normal: rasio kontras minimal 4.5:1 terhadap background.
   - Teks besar dan status indicators: rasio kontras minimal 3:1 (R-25).
5. **Fokus Terminal Otomatis**: Saat tab terminal diaktifkan secara sengaja oleh pengguna, fokus keyboard dialihkan ke terminal xterm.js agar pengguna dapat langsung mengetik.
6. **Isolasi Shortcut Aplikasi**: Shortcut global aplikasi tidak boleh merebut kombinasi tombol standar terminal (misalnya `Ctrl+C`, `Ctrl+D`, `Ctrl+Z`, `Ctrl+A`) saat kursor berada di dalam terminal.
7. **Reduced Motion**: Mendukung media query `prefers-reduced-motion` untuk menonaktifkan atau meminimalkan transisi animasi bagi pengguna yang sensitif.
8. **Touch Target Size**: Ukuran area sentuh minimal 44x44px pada layar sentuh/mobile (R-03).

---

## 9. Design Tokens

### Audit Token shadcn-svelte & Stylesheet Global

Termust menggunakan Tailwind CSS v4 dengan `@theme inline` dan token canonical shadcn-svelte bergaya `nova` / `neutral`. Token yang sudah ada:

- Warna dasar: `--background`, `--foreground`, `--card`, `--popover`, `--primary`, `--secondary`, `--muted`, `--accent`, `--destructive`, `--border`, `--input`, `--ring`.
- Terminal: `--terminal-background`, `--terminal-foreground`.
- Font mono: `--font-mono` (JetBrains Mono, SFMono, Menlo, Consolas).

### Token Fondasi Layout dan Status Tambahan (Phase 1G-0)

Untuk mencegah angka layout sembarangan (magic numbers) dan pewarnaan status yang di-hardcode di banyak file komponen, token semantik berikut ditambahkan ke `src/app.css`:

```css
/* Layout dimensions */
--titlebar-height: 2.75rem; /* 44px: tinggi header/titlebar terpadu */
--sidebar-width: 15rem; /* 240px: lebar default sidebar desktop */
--sidebar-collapsed-width: 3.5rem; /* 56px: lebar icon rail saat sidebar collapse */
--context-panel-width: 20rem; /* 320px: lebar panel detail context kanan */
--terminal-tools-width: 18rem; /* 288px: lebar panel tools terminal */

/* Status indicators (oklch semantic) */
--status-connected: oklch(0.696 0.17 162.48); /* Hijau emerald */
--status-connecting: oklch(0.769 0.188 70.08); /* Kuning amber */
--status-disconnecting: oklch(0.769 0.188 70.08); /* Kuning amber */
--status-error: oklch(0.645 0.246 16.439); /* Merah rose / destructive */
--status-idle: oklch(0.556 0 0); /* Abu-abu netral */
```

Diakses via utility Tailwind: `h-titlebar`, `w-sidebar`, `w-sidebar-collapsed`, `w-context-panel`, `w-terminal-tools`, `bg-status-connected`, `bg-status-connecting`, `bg-status-error`, `bg-status-idle`.

Keputusan arsitektur: Tidak menambahkan tema light terpisah karena produk adalah developer terminal tool dengan fokus dark theme, dan tidak menambahkan runtime dependency baru untuk design token.

---

## 10. Component Boundary Plan

Target struktur komponen untuk sub-phase berikutnya dirancang modular tanpa membuat wrapper berlebihan:

```text
src/lib/components/
├── app-shell/
│   ├── app-shell.svelte          (Kontainer root 3 kolom dan header titlebar)
│   ├── app-titlebar.svelte       (Titlebar terintegrasi dengan tab strip & nav)
│   └── app-sidebar.svelte        (Navigasi samping kiri dengan mode collapse)
├── inventory/
│   ├── host-workspace.svelte     (Workspace inventory: filter, search, grid)
│   ├── host-grid.svelte          (Grid render kumpulan host card)
│   ├── host-card.svelte          (Komponen kartu host mandiri dengan status)
│   └── host-detail-panel.svelte  (Context panel kanan untuk detail host terpilih)
├── terminal/
│   ├── terminal-workspace.svelte (Area kerja terminal dan session pane management)
│   ├── session-tab-strip.svelte  (Tab strip multi-sesi yang sudah ada)
│   └── terminal-tools-panel.svelte (Panel samping kanan untuk quick snippets)
└── shared/
    ├── panel-header.svelte       (Header seragam untuk sidebar dan context panel)
    ├── status-indicator.svelte   (Dot status semantik terpusat)
    └── empty-state.svelte        (Tampilan seragam untuk state tanpa data)
```

### Alasan Pembagian:

1. `host-workspace` memecah beban monolitik `inventory-panel.svelte` (1.190 baris) agar logika kartu host dan panel detail terisolasi.
2. `terminal-workspace` mengisolasi rendering tab terminal dari layout inventory, menjaga invariant xterm tetap utuh.
3. `session-tab-strip` yang sudah teruji pada Phase 1F tetap dipertahankan dan diintegrasikan ke titlebar/header.
4. Tidak membuat generic wrapper satu baris jika elemen HTML biasa sudah mencukupi.

---

## 11. Rencana Sub-Phase (Roadmap Phase 1G)

Setiap sub-phase memiliki scope ketat dan batasan keras yang tidak boleh dilanggar:

### Phase 1G-1: Theme, App Shell, dan Native Title Bar

- **Scope**: Implementasi layout dasar `AppShell`, integrasi titlebar terpadu, sidebar navigasi dengan toggle collapse, dan wiring navigasi workspace.
- **Dilarang**: Mengubah backend Rust, mengubah lifecycle tab, mengubah konfigurasi window Tauri jika belum diperlukan, dan menyentuh form database.

### Phase 1G-2: Inventory Workspace

- **Scope**: Refactor `inventory-panel.svelte` menjadi `host-workspace`, `host-grid`, `host-card`, dan `host-detail-panel`. Pemilihan host card menampilkan detail di ContextPanel.
- **Dilarang**: Mengubah skema vault, mengubah command Tauri untuk host/credential/group, mengubah logika connect saved host, serta menambahkan riwayat koneksi, last connected, uptime, latency, logs, tags, sharing, atau data sintetis yang belum didukung backend.

### Phase 1G-3: Terminal Workspace

- **Scope**: Implementasi `terminal-workspace`, pemindahan `terminal-tools-panel` (snippet runner cepat) ke context panel kanan, integrasi status bar sesi.
- **Dilarang**: Mengubah xterm container lifecycle (`terminal-session.svelte`), mengubah `session-lifecycle.ts`, membuang instance tab saat tidak aktif, serta menambahkan metrik latency, network statistics, command history, autocomplete, atau telemetry palsu yang tidak ada di backend.

### Phase 1G-4: Forms dan Dialogs

- **Scope**: Penataan dialog form (Quick Connect form, Host Create/Edit modal, Credential modal, Import OpenSSH wizard dialog, Backup/Restore wizard dialog).
- **Dilarang**: Menyimpan kredensial ke state global atau local storage, mengubah payload validasi IPC.

### Phase 1G-5: Responsive dan Mobile Layout

- **Scope**: Penyesuaian layout untuk tablet (sidebar rail, sheet context) dan mobile (drawer navigasi, single-column host list, full-screen terminal, touch target minimum 44px).
- **Dilarang**: Menambah dependency CSS eksternal baru atau mengubah breakpoint standar Tailwind.

### Phase 1G-6: Accessibility dan Hardening

- **Scope**: Audit keyboard navigation menyeluruh, perbaikan focus-visible, penambahan accessible label/tooltip pada icon-only buttons, pengujian high contrast dan reduced motion.
- **Dilarang**: Mengubah alur bisnis atau protokol SSH.

### Phase 1G-7: Final Visual QA

- **Scope**: Review visual menyeluruh, verifikasi polish micro-interaction, verifikasi konsistensi warna status, dan checklist pengujian manual.
- **Dilarang**: Menambah fitur baru di luar cakupan Phase 1G.
