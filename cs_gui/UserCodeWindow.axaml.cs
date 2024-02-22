using System;
using System.Diagnostics;
using System.Threading;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Threading;

namespace cs_gui;

public partial class UserCodeWindow : Window {
    private readonly string _userCode;
    private readonly string _verificationUrl;
    private readonly CancellationTokenSource _cancelToken = new ();
    private readonly SafeNativeMethods _handle;
    
    public UserCodeWindow(SafeNativeMethods handle, string userCode, string verificationUrl) {
        _handle = handle;
        _userCode = userCode;
        _verificationUrl = verificationUrl;

        InitializeComponent();

        Closing += (_, _) =>
        {
            _cancelToken.Cancel();
        };
        
        UserCodeDisplay.Text = _userCode;
    }

    private void InputElement_OnTapped(object? _, TappedEventArgs e) {
        Clipboard?.SetTextAsync(_userCode);
    }

    private void UserCodeDisplay_OnPointerEntered(object? _, PointerEventArgs e) {
        UserCodeDisplay.Foreground = Brushes.CornflowerBlue;
        UserCodeDisplay.TextDecorations = TextDecorations.Underline;
    }

    private void UserCodeDisplay_OnPointerExited(object? _, PointerEventArgs e) {
        UserCodeDisplay.Foreground = Foreground;
        UserCodeDisplay.TextDecorations = null;
    }

    private void UserUrl_OnTapped(object? _, TappedEventArgs e) {
        var startInfo = new ProcessStartInfo {
            FileName = _verificationUrl,
            UseShellExecute = true
        };

        if (Process.Start(startInfo) == null) {
            throw new NullReferenceException();
        };

        Dispatcher.UIThread.InvokeAsync(async () => {
            await _handle.Auth(_cancelToken.Token);
            Close();
        });
    }

    private void UserUrl_OnPointerEntered(object? _, PointerEventArgs e) {
        UserUrl.Foreground = Brushes.CornflowerBlue;
        UserUrl.TextDecorations = TextDecorations.Underline;
    }

    private void UserUrl_OnPointerExited(object? _, PointerEventArgs e) {
        UserUrl.Foreground = Foreground;
        UserUrl.TextDecorations = null;
    }
}