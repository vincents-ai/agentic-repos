document.addEventListener('DOMContentLoaded', function() {
    console.log('agentic-repos Web UI loaded');
    
    const copyButtons = document.querySelectorAll('button[onclick*="copyToClipboard"]');
    copyButtons.forEach(button => {
        button.addEventListener('click', function(e) {
            e.preventDefault();
            const text = this.getAttribute('onclick').match(/'([^']+)'/)[1];
            navigator.clipboard.writeText(text).then(() => {
                const originalText = this.textContent;
                this.textContent = 'Copied!';
                this.style.backgroundColor = 'var(--success-color)';
                
                setTimeout(() => {
                    this.textContent = originalText;
                    this.style.backgroundColor = '';
                }, 2000);
            }).catch(err => {
                console.error('Failed to copy: ', err);
                this.textContent = 'Failed';
                setTimeout(() => {
                    this.textContent = 'Copy';
                }, 2000);
            });
        });
    });
});