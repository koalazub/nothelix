document.addEventListener('keydown', function (e) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
    var input = document.getElementById('search-input');
    if (input) {
      e.preventDefault();
      input.focus();
      input.select();
    }
  }
});
document.addEventListener('DOMContentLoaded', function () {
  var input = document.getElementById('search-input');
  if (input) {
    var isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
    input.placeholder = 'Search nothelix… (' + (isMac ? '⌘' : 'Ctrl+') + 'K)';
  }
});
