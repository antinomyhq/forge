# frozen_string_literal: true

class User
  include ActiveModel::Validations
  
  validates :username, presence: true, uniqueness: { case_sensitive: false }, length: { minimum: 3, maximum: 50 }
  validates :email, presence: true, uniqueness: { case_sensitive: false }, format: { with: URI::MailTo::EMAIL_REGEXP }
  validates :age, presence: true, numericality: { only_integer: true }, inclusion: { in: 18..120 }
  validates :first_name, presence: true, length: { maximum: 50 }
  validates :last_name, presence: true, length: { maximum: 50 }
  
  before_validation :normalize_email
  
  has_many :posts, dependent: :destroy
  has_one :profile, dependent: :destroy
  has_many :comments, through: :posts
  
  def full_name
    "#{first_name} #{last_name}"
  end
  
  def display_name
    username || full_name || 'Anonymous'
  end
  
  def admin?
    email&.end_with?('@admin.com')
  end
  
  def teenage?
    age&.between?(13, 19)
  end
  
  private
  
  def normalize_email
    self.email = email&.downcase&.strip if email&.present?
  end
end

class Profile < ApplicationRecord
  belongs_to :user
  validates :bio, length: { maximum: 500 }
  validates :website, format: { with: URI::regexp(%r{^https?://.+}) }, allow_blank: true
  validates :github_username, length: { maximum: 50 }, format: { with: /\A[a-zA-Z0-9_-]+\z/ }, allow_blank: true
  validates :twitter_username, length: { maximum: 50 }, format: { with: /\A@[a-zA-Z0-9_]+\z/ }, allow_blank: true
end

class Post < ApplicationRecord
  include ActiveModel::Validations
  include ActiveModel::Sluggable
  
  validates :title, presence: true, length: { maximum: 255 }
  validates :slug, presence: true, uniqueness: true, length: { maximum: 255 }
  validates :content, presence: true, length: { minimum: 10 }
  validates :status, inclusion: { in: %w[draft published archived] }
  
  belongs_to :author, class_name: 'User', foreign_key: 'author_id'
  has_many :comments, dependent: :destroy
  has_and_belongs_to_many :tags, through: :post_tags
  
  scope :published, -> { where(status: 'published') }
  scope :recent, -> { order(created_at: :desc) }
  scope :by_author, ->(author) { where(author_id: author.id) }
  
  def excerpt_truncated
    excerpt&.truncate(100) if excerpt&.present?
  end
  
  def to_param
    slug
  end
  
  def published?
    status == 'published'
  end
  
  def draft?
    status == 'draft'
  end
end

class Tag < ApplicationRecord
  validates :name, presence: true, uniqueness: true, length: { maximum: 50 }
  validates :color, format: { with: /\A#[0-9A-Fa-f]{6}\z/ }, allow_blank: true
  
  has_and_belongs_to_many :posts, through: :post_tags
  
  def to_param
    name&.parameterize
  end
end

class PostTag < ApplicationRecord
  belongs_to :post
  belongs_to :tag
  
  validates :post_id, uniqueness: { scope: :tag_id }
  validates :tag_id, uniqueness: { scope: :post_id }
end

class Comment < ApplicationRecord
  include ActiveModel::Validations
  
  validates :content, presence: true, length: { minimum: 5, maximum: 1000 }
  validates :author, presence: true
  
  belongs_to :post
  belongs_to :author, class_name: 'User', foreign_key: 'author_id'
  belongs_to :parent_comment, class_name: 'Comment', optional: true, foreign_key: 'parent_comment_id'
  
  has_many :replies, class_name: 'Comment', foreign_key: 'parent_comment_id', dependent: :destroy
  
  scope :approved, -> { where(is_approved: true) }
  scope :recent, -> { order(created_at: :desc) }
  
  def approved?
    is_approved?
  end
  
  def pending?
    !is_approved?
  end
  
  def root?
    parent_comment_id.nil?
  end
  
  def reply?
    parent_comment_id.present?
  end
end

# Invalid Ruby: Missing end keyword for User class
def invalid_method
  puts "This method is missing an end keyword"
    puts "Nested without proper indentation"