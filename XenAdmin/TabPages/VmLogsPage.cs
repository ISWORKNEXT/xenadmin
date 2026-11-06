/* Copyright (c) ISWORKNEXT
 *
 * Licensed under the BSD 2-Clause license, the same terms as the rest of
 * this repository.
 */

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Windows.Forms;
using XenAdmin.Actions;
using XenAPI;

namespace XenAdmin.TabPages
{
    public class VmLogsPage : BaseTabPage
    {
        private static readonly log4net.ILog log = log4net.LogManager.GetLogger(System.Reflection.MethodBase.GetCurrentMethod().DeclaringType);

        private readonly DataGridView _grid;
        private readonly Button _refresh;
        private readonly CollectionChangeEventHandler _messagesChanged;
        private readonly List<XenAPI.Message> _shown = new List<XenAPI.Message>();
        private VM _vm;
        private int _loadGeneration;

        public VmLogsPage()
        {
            Text = "Logs";
            _messagesChanged = Program.ProgramInvokeHandler(MessagesChanged);

            _grid = new DataGridView
            {
                Dock = DockStyle.Fill,
                ReadOnly = true,
                AllowUserToAddRows = false,
                AllowUserToDeleteRows = false,
                AllowUserToResizeRows = false,
                RowHeadersVisible = false,
                MultiSelect = false,
                SelectionMode = DataGridViewSelectionMode.FullRowSelect,
                AutoSizeRowsMode = DataGridViewAutoSizeRowsMode.AllCells,
                BackgroundColor = System.Drawing.SystemColors.Window,
                BorderStyle = BorderStyle.None
            };
            _grid.DefaultCellStyle.WrapMode = DataGridViewTriState.True;
            _grid.Columns.Add(new DataGridViewTextBoxColumn
            {
                HeaderText = "Date",
                Name = "Date",
                AutoSizeMode = DataGridViewAutoSizeColumnMode.AllCells
            });
            _grid.Columns.Add(new DataGridViewTextBoxColumn
            {
                HeaderText = "Name",
                Name = "Name",
                AutoSizeMode = DataGridViewAutoSizeColumnMode.AllCells
            });
            _grid.Columns.Add(new DataGridViewTextBoxColumn
            {
                HeaderText = "Content",
                Name = "Content",
                AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill
            });
            var bar = new FlowLayoutPanel
            {
                Dock = DockStyle.Top,
                AutoSize = true,
                WrapContents = false,
                Padding = new Padding(0, 0, 0, 6)
            };
            _refresh = new Button
            {
                Text = "Refresh",
                AutoSize = true,
                UseVisualStyleBackColor = true
            };
            _refresh.Click += (sender, args) => Reload();
            bar.Controls.Add(_refresh);
            pageContainerPanel.Controls.Add(_grid);
            pageContainerPanel.Controls.Add(bar);
        }

        public override string HelpID => "TabPageLogs";

        public VM VM
        {
            set
            {
                if (_vm != null)
                    _vm.Connection.Cache.DeregisterCollectionChanged<XenAPI.Message>(_messagesChanged);

                _vm = value;
                _shown.Clear();
                _grid.Rows.Clear();
                if (_vm == null)
                    return;

                _vm.Connection.Cache.RegisterCollectionChanged<XenAPI.Message>(_messagesChanged);
                Reload();
            }
        }

        private void MessagesChanged(object sender, CollectionChangeEventArgs e)
        {
            if (!Visible || _vm == null)
                return;
            Merge(CacheMessages(), false);
        }

        private void Reload()
        {
            if (_vm == null)
                return;

            Merge(CacheMessages(), false);
            _refresh.Enabled = false;

            var vm = _vm;
            var generation = ++_loadGeneration;
            var since = DateTime.UtcNow.AddDays(-400);
            var action = new DelegatedAsyncAction(vm.Connection, "Load VM logs", "Loading VM logs", "Loaded VM logs",
                session =>
                {
                    Dictionary<XenRef<XenAPI.Message>, XenAPI.Message> found;
                    try
                    {
                        found = XenAPI.Message.get(session, cls.VM, vm.uuid, since);
                    }
                    catch (Exception ex)
                    {
                        log.Debug("VM log query failed.", ex);
                        return;
                    }

                    var rows = found == null
                        ? new List<XenAPI.Message>()
                        : found.Values.Where(message => message != null).OrderByDescending(message => message.timestamp).ToList();
                    foreach (var message in rows)
                        message.Connection = vm.Connection;
                    Program.Invoke(this, () =>
                    {
                        if (generation != _loadGeneration || _vm == null || _vm.uuid != vm.uuid)
                            return;
                        if (rows.Count > 0)
                            Merge(rows, true);
                        _refresh.Enabled = true;
                    });
                },
                true,
                "message.get");
            action.Completed += sender =>
            {
                if (!IsHandleCreated)
                    return;
                Program.Invoke(this, () => _refresh.Enabled = true);
            };
            action.RunAsync();
        }

        private void Merge(IEnumerable<XenAPI.Message> incoming, bool replace)
        {
            var map = new Dictionary<string, XenAPI.Message>();
            if (!replace)
            {
                foreach (var message in _shown)
                    map[MessageKey(message)] = message;
            }

            if (incoming != null)
            {
                foreach (var message in incoming)
                {
                    if (message == null)
                        continue;
                    map[MessageKey(message)] = message;
                }
            }

            _shown.Clear();
            _shown.AddRange(map.Values.OrderByDescending(message => message.timestamp));
            Render();
        }

        private static string MessageKey(XenAPI.Message message)
        {
            if (!string.IsNullOrEmpty(message.opaque_ref) && message.opaque_ref != "OpaqueRef:NULL")
                return message.opaque_ref;
            return message.timestamp.Ticks + "|" + message.name + "|" + message.body;
        }

        private List<XenAPI.Message> CacheMessages()
        {
            if (_vm == null)
                return new List<XenAPI.Message>();

            return _vm.Connection.Cache.Messages
                .Where(message => message != null && message.obj_uuid == _vm.uuid)
                .OrderByDescending(message => message.timestamp)
                .ToList();
        }

        private void Render()
        {
            var rows = new List<object[]>();
            foreach (var message in _shown)
            {
                if (message == null)
                    continue;
                rows.Add(new object[]
                {
                    FormatTimestamp(message),
                    message.name ?? "",
                    message.body ?? ""
                });
            }

            _grid.Rows.Clear();
            foreach (var row in rows)
                _grid.Rows.Add(row);
        }

        private static string FormatTimestamp(XenAPI.Message message)
        {
            if (message.Connection != null)
                return message.TimestampLocal().ToString("yyyy-MM-dd HH:mm");

            var stamp = message.timestamp;
            if (stamp.Kind == DateTimeKind.Unspecified)
                stamp = DateTime.SpecifyKind(stamp, DateTimeKind.Utc);
            return stamp.ToLocalTime().ToString("yyyy-MM-dd HH:mm");
        }
    }
}
