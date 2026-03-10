import { Search, Filter } from 'lucide-react'

export default function Sidebar() {
  return (
    <aside className="w-80 bg-gray-900 border-r border-gray-800 flex flex-col">
      {/* Sidebar Header */}
      <div className="p-4 border-b border-gray-800">
        <h2 className="text-lg font-bold mb-3">Plugin Library</h2>
        
        {/* Search */}
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-500" />
          <input
            type="text"
            placeholder="Search plugins..."
            className="w-full pl-10 pr-4 py-2 bg-gray-800 border border-gray-700 rounded-lg focus:outline-none focus:ring-2 focus:ring-primary-500"
          />
        </div>
      </div>

      {/* Filter Tabs */}
      <div className="flex gap-2 p-4 border-b border-gray-800">
        <button className="flex-1 px-3 py-2 bg-primary-600 rounded-lg text-sm font-medium">
          All
        </button>
        <button className="flex-1 px-3 py-2 bg-gray-800 hover:bg-gray-700 rounded-lg text-sm font-medium transition-colors">
          VST3
        </button>
        <button className="flex-1 px-3 py-2 bg-gray-800 hover:bg-gray-700 rounded-lg text-sm font-medium transition-colors">
          CLAP
        </button>
      </div>

      {/* Plugin List */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="space-y-2">
          {/* Empty State */}
          <div className="text-center py-12 text-gray-500">
            <Filter className="w-12 h-12 mx-auto mb-3 opacity-50" />
            <p className="text-sm">No plugins found</p>
            <p className="text-xs mt-1">Scan for plugins in settings</p>
          </div>
        </div>
      </div>

      {/* Sidebar Footer */}
      <div className="p-4 border-t border-gray-800">
        <button className="w-full px-4 py-2 bg-gray-800 hover:bg-gray-700 rounded-lg text-sm font-medium transition-colors">
          Scan for Plugins
        </button>
      </div>
    </aside>
  )
}
