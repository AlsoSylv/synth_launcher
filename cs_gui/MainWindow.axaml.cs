using System;
using System.Collections.ObjectModel;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Threading;

namespace cs_gui;

public partial class MainWindow : Window
{
    private Task? _versionTask;
    private readonly CancellationTokenSource _token = new();
    private readonly ObservableCollection<string> _accounts;
    private readonly ObservableCollection<string> _jvms;
    private SafeNativeMethods _handle;

    public MainWindow()
    {
        _handle = new SafeNativeMethods();
        _accounts = new ObservableCollection<string>();
        _jvms = new ObservableCollection<string> { "Default" };

        InitializeComponent();
        var task = _handle.GetManifest();
        
        AccountSelector.ItemsSource = _accounts;
        JvmSelector.ItemsSource = _jvms;
        JvmSelector.SelectedIndex = 0;

        for (nuint i = 0; i < _handle.AccountLength; i++) _accounts.Add(_handle.GetAccountName(i));
        for (nuint i = 0; i < _handle.JvmLen; i++) _jvms.Add(_handle.GetAccountName(i));
        
        VersionSelectBox.IsEnabled = false;

        Dispatcher.UIThread.InvokeAsync(async () =>
        {
            try
            {
                await task;
                var list = new ObservableCollection<string>();
                VersionSelectBox.ItemsSource = list;
                var len = _handle.ManifestLength();
                
                for (UIntPtr idx = 0; idx < len; idx++)
                    list.Add(Encoding.UTF8.GetString(_handle.GetVersionId(idx)));
                
                VersionSelectBox.IsEnabled = true;
            }
            catch (AggregateException ae)
            {
                ae.Handle(x =>
                {
                    if (x is not RustException) return false;
                    Console.WriteLine(x);
                    return true;
                });
            }
        });
    }
    
    private void VersionSelectBox_OnSelectionChanged(object? sender, SelectionChangedEventArgs _) {
        var versionBox = (ComboBox)sender!;
        if (_versionTask == null)
        {
            var index = versionBox.SelectedIndex;
            _versionTask = _handle.GetVersion((nuint)index, _token.Token);
        }
        else
        {
            if (!_versionTask.IsCompleted) _token.Cancel();
            _token.TryReset();

            var index = versionBox.SelectedIndex;
            _versionTask = _handle.GetVersion((nuint)index, _token.Token);
        }
    }

    private void Button_OnClick(object? sender, RoutedEventArgs _) {
        var button = (Button) sender!;
        button.IsEnabled = false;

        Dispatcher.UIThread.InvokeAsync(async delegate {
            await _handle.GetDeviceResponse;
            var userCode = _handle.GetCode();
            var window = new UserCodeWindow(_handle, userCode, _handle.GetUrl()) {
                UserCodeDisplay = {
                    Text = userCode
                }
            };

            var res = await window.ShowDialog<string>(this);

            if (res == "success") _accounts.Add(_handle.GetAccountName(_handle.AccountLength - 1));

            button.IsEnabled = true;
        });
    }

    private void PlayButton_OnClick(object? sender, RoutedEventArgs e) {
        var button = (Button)sender!;
        if (VersionSelectBox.SelectedIndex < 0 | _versionTask == null) return;
        button.IsEnabled = false;
        
        Dispatcher.UIThread.InvokeAsync(async delegate {
            await _versionTask!;
            var assetTask = new AssetTask(ref _handle);
            var librariesTask = new LibrariesTask(ref _handle);
            var jarTask = new JarTask(ref _handle);
            
            var progressWindow = new ProgressDialog();
            progressWindow.Show();
            
            var task = Task.WhenAll([assetTask.Task, librariesTask.Task, jarTask.Task]);
            while (!task.IsCompleted) {
                if (librariesTask.Total != 0) progressWindow.LibraryProgressBar.Value = librariesTask.Percentage;
                if (assetTask.Total != 0) progressWindow.AssetProgressBar.Value = assetTask.Percentage;
                if (jarTask.Total != 0) progressWindow.JarProgressBar.Value = jarTask.Percentage;
                await Task.Delay(10);
            }
            
            progressWindow.LibraryProgressBar.Value = librariesTask.Percentage;
            progressWindow.AssetProgressBar.Value = assetTask.Percentage;
            progressWindow.JarProgressBar.Value = jarTask.Percentage;
            
            var jvmIndex = JvmSelector.SelectedIndex;
            var accIndex = AccountSelector.SelectedIndex;
            if (jvmIndex == 0) 
                _handle.Play((nuint)accIndex);
            else
                _handle.Play((nuint)jvmIndex-1, (nuint)accIndex);
            button.IsEnabled = true;
        });
    }

    private void AccountSelector_OnSelectionChanged(object? sender, SelectionChangedEventArgs _) {
        var box = (ComboBox)sender!;
        var index = (nuint) box.SelectedIndex;
        if (!_handle.NeedsRefresh(index)) return;
        
        box.IsEnabled = false;
        Dispatcher.UIThread.InvokeAsync(async () => {
            var refreshTask = _handle.RefreshAccount(index);
            while (!refreshTask.IsCompleted) {
                await Task.Delay(10);
            }

            try {
                await refreshTask;
                box.IsEnabled = true;
            }
            catch (Exception ex) {
                Console.WriteLine(ex);
            }
        });
    }

    private void RemoveAccount_OnClick(object? sender, RoutedEventArgs e) {
        var index = AccountSelector.SelectedIndex;
        if (index < 0) return;
        _accounts.RemoveAt(index);
        _handle.RemoveAccount((nuint) index);
    }
}