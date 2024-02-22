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
    private SafeNativeMethods _handle;

    public MainWindow()
    {
        _handle = new SafeNativeMethods();
        _accounts = new ObservableCollection<string>();
        
        InitializeComponent();
        var task = _handle.GetManifest();
        
        AccountSelector.ItemsSource = _accounts;

        for (nuint i = 0; i < _handle.AccountLength; i++) _accounts.Add(_handle.GetAccountName(i));
        

        VersionSelectBox.IsEnabled = false;
        LibraryProgressBar.Minimum = 0;
        LibraryProgressBar.Maximum = 1;
        LibraryProgressBar.ShowProgressText = true;

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
    
    private void VersionSelectBox_OnSelectionChanged(object? _, SelectionChangedEventArgs e)
    {
        if (_versionTask == null)
        {
            var index = VersionSelectBox.SelectedIndex;
            _versionTask = _handle.GetVersion((nuint)index, _token.Token);
        }
        else
        {
            if (!_versionTask.IsCompleted) _token.Cancel();
            _token.TryReset();

            var index = VersionSelectBox.SelectedIndex;
            _versionTask = _handle.GetVersion((nuint)index, _token.Token);
        }
    }

    private void Button_OnClick(object? sender, RoutedEventArgs _) {
        var button = (Button) sender!;
        button.IsEnabled = false;

        Dispatcher.UIThread.InvokeAsync(async () =>
        {
            await _handle.GetDeviceResponse;
            var window = new UserCodeWindow(_handle, _handle.GetCode(), _handle.GetUrl());

            window.Show();

            window.Closed += delegate {
                _accounts.Add(_handle.GetAccountName(_handle.AccountLength - 1));
                
                button.IsEnabled = true;
            };
        });
    }

    private void PlayButton_OnClick(object? sender, RoutedEventArgs e) {
        Dispatcher.UIThread.InvokeAsync(async () => {
            await _versionTask!;
            var assetTask = new AssetTask(ref _handle);
            while (!assetTask.Task.IsCompleted) {
                if (assetTask.Total != 0) {
                    LibraryProgressBar.Value = assetTask.Percentage;
                }
                await Task.Delay(10);
            }
            
            LibraryProgressBar.Value = assetTask.Percentage;
        });
    }

    private void AccountSelector_OnSelectionChanged(object? sender, SelectionChangedEventArgs e) {
        var box = (ComboBox)sender!;
        var index = (nuint) box.SelectedIndex;
        if (!_handle.NeedsRefresh(index)) return;
        
        box.IsEnabled = false;
        Dispatcher.UIThread.InvokeAsync(async () => {
            var refreshTask = _handle.RefreshAccount(index);
            while (!refreshTask.IsCompleted) {
                Console.WriteLine("Waiting...");
                await Task.Delay(10);
            }

            try {
                await refreshTask;
                box.IsEnabled = true;
            }
            catch (Exception e) {
                Console.WriteLine(e);
            }
        });
    }
}