import { invoke } from '@tauri-apps/api/tauri';
import { open } from '@tauri-apps/api/dialog';

// DOM elements
const authOverlay = document.getElementById('auth-overlay');
const mainUI = document.getElementById('main-ui');
const authSubmit = document.getElementById('auth-submit');
const authPassword = document.getElementById('auth-password');
const authError = document.getElementById('auth-error');
const bookListEl = document.getElementById('book-list');
const importBtn = document.getElementById('import-btn');
const batchImportBtn = document.getElementById('batch-import-btn');
const adminModal = document.getElementById('admin-modal');
const aboutModal = document.getElementById('about-modal');
const readerContent = document.getElementById('reader-content');
const prevPageBtn = document.getElementById('prev-page');
const nextPageBtn = document.getElementById('next-page');
const pageIndicator = document.getElementById('page-indicator');
const fontSelect = document.getElementById('font-select');
const fontSizeSlider = document.getElementById('font-size');
const themeToggle = document.getElementById('theme-toggle');
const focusBtn = document.getElementById('focus-mode');
const searchToggle = document.getElementById('search-toggle');
const searchBar = document.getElementById('search-bar');
const searchInput = document.getElementById('search-input');
const searchPrev = document.getElementById('search-prev');
const searchNext = document.getElementById('search-next');
const searchCounter = document.getElementById('search-counter');
const bookmarkBtn = document.getElementById('bookmark-btn');
const notesBtn = document.getElementById('notes-btn');
const highlightBtn = document.getElementById('highlight-btn');
const dictBtn = document.getElementById('dict-btn');
const totalBooksSpan = document.getElementById('total-books');
const totalReadingTimeSpan = document.getElementById('total-reading-time');

let currentBook = null;
let currentPage = 1;
let totalPages = 1;
let isFocusMode = false;
let searchMatches = [];
let currentMatch = 0;

async function checkAuth() {
    const needsAuth = await invoke('needs_authentication');
    if (needsAuth) {
        authOverlay.classList.remove('hidden');
        mainUI.classList.add('hidden');
    } else {
        authOverlay.classList.add('hidden');
        mainUI.classList.remove('hidden');
        loadLibrary();
    }
}

authSubmit.addEventListener('click', async () => {
    const pwd = authPassword.value;
    const success = await invoke('verify_password', { password: pwd });
    if (success) {
        authOverlay.classList.add('hidden');
        mainUI.classList.remove('hidden');
        loadLibrary();
    } else {
        authError.classList.remove('hidden');
    }
});

async function loadLibrary() {
    const books = await invoke('get_library');
    bookListEl.innerHTML = '';
    books.forEach(book => {
        const li = document.createElement('li');
        li.textContent = `${book.title} (${book.progress}%)`;
        li.dataset.id = book.id;
        li.addEventListener('click', () => openBook(book.id));
        bookListEl.appendChild(li);
    });
    totalBooksSpan.textContent = books.length;
    const stats = await invoke('get_reading_stats');
    totalReadingTimeSpan.textContent = stats.totalMinutes;
}

async function openBook(bookId) {
    currentBook = await invoke('open_book', { id: bookId });
    currentPage = currentBook.current_page;
    totalPages = currentBook.total_pages;
    renderPage();
    document.getElementById('reader-toolbar').classList.remove('hidden');
}

async function renderPage() {
    const content = await invoke('get_page_content', {
        bookId: currentBook.id,
        page: currentPage,
        fontSize: fontSizeSlider.value,
        fontFamily: fontSelect.value
    });
    readerContent.innerHTML = content;
    pageIndicator.textContent = `Page ${currentPage} / ${totalPages}`;
    await invoke('save_progress', { bookId: currentBook.id, page: currentPage });
}

prevPageBtn.onclick = () => { if (currentPage > 1) { currentPage--; renderPage(); } };
nextPageBtn.onclick = () => { if (currentPage < totalPages) { currentPage++; renderPage(); } };
fontSelect.onchange = renderPage;
fontSizeSlider.oninput = renderPage;

themeToggle.onclick = () => {
    document.body.classList.toggle('dark-mode');
    themeToggle.textContent = document.body.classList.contains('dark-mode') ? '☀️ Light' : '🌙 Dark';
};
focusBtn.onclick = () => {
    isFocusMode = !isFocusMode;
    const reader = document.querySelector('.reader-panel');
    reader.classList.toggle('focus-mode', isFocusMode);
    document.getElementById('focus-overlay').classList.toggle('hidden', !isFocusMode);
};

searchToggle.onclick = () => searchBar.classList.toggle('hidden');
async function performSearch() {
    const query = searchInput.value.trim();
    if (!query) return;
    searchMatches = await invoke('search_in_book', { bookId: currentBook.id, query });
    currentMatch = 0;
    if (searchMatches.length) {
        searchCounter.textContent = `${currentMatch+1}/${searchMatches.length}`;
        jumpToMatch();
    } else {
        searchCounter.textContent = 'No results';
    }
}
searchInput.addEventListener('input', performSearch);
searchPrev.onclick = () => { if (searchMatches.length) { currentMatch = (currentMatch-1+searchMatches.length)%searchMatches.length; jumpToMatch(); }};
searchNext.onclick = () => { if (searchMatches.length) { currentMatch = (currentMatch+1)%searchMatches.length; jumpToMatch(); }};
async function jumpToMatch() {
    const { page, context } = searchMatches[currentMatch];
    currentPage = page;
    await renderPage();
    searchCounter.textContent = `${currentMatch+1}/${searchMatches.length}`;
}

bookmarkBtn.onclick = async () => {
    await invoke('add_bookmark', { bookId: currentBook.id, page: currentPage, note: '' });
    alert('Bookmark added');
};
notesBtn.onclick = async () => {
    const note = prompt('Enter your note:');
    if (note) await invoke('add_note', { bookId: currentBook.id, page: currentPage, note });
};
highlightBtn.onclick = async () => {
    const selection = window.getSelection().toString();
    if (selection) await invoke('add_highlight', { bookId: currentBook.id, page: currentPage, text: selection });
    else alert('Select some text first');
};
dictBtn.onclick = async () => {
    const word = window.getSelection().toString();
    if (word) {
        const definition = await invoke('lookup_word', { word });
        alert(definition || 'No definition found');
    } else alert('Select a word');
};

importBtn.onclick = async () => {
    const selected = await open({ filters: [{ name: 'Books', extensions: ['epub', 'pdf'] }] });
    if (selected) {
        await invoke('import_book', { path: selected });
        loadLibrary();
    }
};
batchImportBtn.onclick = () => adminModal.classList.remove('hidden');
document.querySelectorAll('.modal .close').forEach(btn => {
    btn.onclick = () => {
        adminModal.classList.add('hidden');
        aboutModal.classList.add('hidden');
    };
});
document.getElementById('select-folder-btn').onclick = async () => {
    const folder = await open({ directory: true });
    if (folder) {
        const files = await invoke('scan_folder_for_books', { folder });
        const logDiv = document.getElementById('import-log');
        logDiv.innerHTML = files.map(f => `<div>${f}</div>`).join('');
        document.getElementById('start-import').disabled = false;
        document.getElementById('start-import').onclick = async () => {
            await invoke('batch_import', { folder });
            loadLibrary();
            adminModal.classList.add('hidden');
        };
    }
};
document.getElementById('about-btn').onclick = () => aboutModal.classList.remove('hidden');

checkAuth(); 
